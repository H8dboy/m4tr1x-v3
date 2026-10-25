//! Feed con un trasporto finto (niente rete, niente Tor, tempo virtuale): eventi verificati, tentativi
//! fino al budget, fase segnalata. Gli eventi sono i vettori CONGELATI di packages/protocol.

use m4tr1x_feed::{leggi, leggi_con_ripresa, Fase, ATTESA_TRA_TENTATIVI, BUDGET, RIPRESA};
use m4tr1x_protocol::Rete;
use m4tr1x_transport::prova::{relay_finto_che_ignora_il_filtro, relay_finto_condiviso, CONTENUTO_RIFIUTATO};
use m4tr1x_transport::{BootstrapInfo, Isolation, OnionV3, Stream, Transport, TransportError, TransportStatus, TEMPO_MASSIMO_CONNESSIONE};
use serde_json::Value;
use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

const VETTORI: &str = include_str!("../../../packages/protocol/vectors/eventi.json");

fn vettori() -> Vec<(bool, Value)> {
    let f: Value = serde_json::from_str(VETTORI).unwrap();
    f["vettori"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let mut e = v["evento"].clone();
            if let Some(parti) = e["sig"].as_array() {
                e["sig"] = Value::String(parti.iter().map(|p| p.as_str().unwrap()).collect());
            }
            (v["valido"].as_bool().unwrap(), e)
        })
        .collect()
}

/// Trasporto finto: i primi `falliscono` tentativi scadono dopo 30 s come quelli veri; `sempre_appeso`
/// non riesce mai; poi un relay finto che manda `eventi` (condivisi tra le connessioni: un post
/// pubblicato si rilegge alla connessione dopo, come su un nodo vero).
struct Finto {
    eventi: Arc<Mutex<Vec<Value>>>,
    falliscono: u32,
    sempre_appeso: bool,
    chiamate: AtomicU32,
    /// relay malevolo: ignora i filtri della REQ e manda ogni evento (kind/autori non chiesti compresi)
    ignora_filtro: bool,
}

impl Finto {
    fn new(eventi: Vec<Value>, falliscono: u32, sempre_appeso: bool) -> Self {
        Self { eventi: Arc::new(Mutex::new(eventi)), falliscono, sempre_appeso, chiamate: AtomicU32::new(0), ignora_filtro: false }
    }
    /// Nodo con un relay MALEVOLO: risponde subito ma ignora i filtri (manda tutto).
    fn malevolo(eventi: Vec<Value>) -> Self {
        Self { eventi: Arc::new(Mutex::new(eventi)), falliscono: 0, sempre_appeso: false, chiamate: AtomicU32::new(0), ignora_filtro: true }
    }
}

impl Transport for Finto {
    fn bootstrap(&self) -> impl Future<Output = m4tr1x_transport::Result<BootstrapInfo>> + Send {
        async { Ok(BootstrapInfo { millisecondi: 0, implementazione: "finto" }) }
    }
    fn connect_onion(&self, _h: &OnionV3, _p: u16, _i: Isolation) -> impl Future<Output = m4tr1x_transport::Result<Stream>> + Send {
        let n = self.chiamate.fetch_add(1, Ordering::SeqCst) + 1;
        let (eventi, falliscono, appeso, ignora) = (self.eventi.clone(), self.falliscono, self.sempre_appeso, self.ignora_filtro);
        async move {
            if appeso || n <= falliscono {
                tokio::time::sleep(TEMPO_MASSIMO_CONNESSIONE).await;
                return Err(TransportError::Connessione("tempo massimo del tentativo superato (30 s)".into()));
            }
            let (client, server) = tokio::io::duplex(1 << 20);
            if ignora {
                tokio::spawn(relay_finto_che_ignora_il_filtro(server, eventi));
            } else {
                tokio::spawn(relay_finto_condiviso(server, eventi));
            }
            Ok(Box::pin(client) as Stream)
        }
    }
    fn status(&self) -> TransportStatus {
        TransportStatus::Pronto
    }
    fn shutdown(&self) -> impl Future<Output = ()> + Send {
        async {}
    }
}

fn nodo() -> OnionV3 {
    OnionV3::from_pubkey([9; 32])
}

#[tokio::test]
async fn solo_post_verificati_dal_piu_recente() {
    let v = vettori();
    let mut eventi: Vec<Value> = v.iter().map(|(_, e)| e.clone()).collect();
    eventi.push(v.iter().find(|(ok, e)| *ok && e["kind"] == 1).unwrap().1.clone()); // un doppione
    let attesi = v.iter().filter(|(ok, e)| *ok && e["kind"] == 1).count();
    let fasi = Mutex::new(vec![]);
    let f = leggi(&Finto::new(eventi, 0, false), &nodo(), Rete::Prova, BUDGET, |x| fasi.lock().unwrap().push(x)).await.unwrap();
    assert_eq!(f.post.len(), attesi, "post verificati");
    // il nodo (come il relay finto) manda solo i kind 1 chiesti: scartati = kind 1 NON validi
    let falsi_kind1 = v.iter().filter(|(ok, e)| !*ok && e["kind"] == 1).count();
    assert_eq!(f.scartati, falsi_kind1, "kind 1 non validi");
    assert!(f.post.windows(2).all(|w| w[0].creato >= w[1].creato), "non in ordine dal più recente");
    assert_eq!(f.tentativi, 1);
    let fasi = fasi.into_inner().unwrap();
    assert!(matches!(fasi.first(), Some(Fase::CercoIlNodo { tentativo: 1, .. })), "{fasi:?}");
    assert!(fasi.iter().any(|x| matches!(x, Fase::Scarico { .. })), "{fasi:?}");
    assert!(matches!(fasi.last(), Some(Fase::Fatto { tentativo: 1, .. })), "{fasi:?}");
}

#[tokio::test]
async fn nessun_testo_di_eventi_non_validi_arriva_al_feed() {
    let v = vettori();
    let falsi: Vec<Value> = v.iter().filter(|(ok, _)| !ok).map(|(_, e)| e.clone()).collect();
    let f = leggi(&Finto::new(falsi.clone(), 0, false), &nodo(), Rete::Prova, BUDGET, |_| {}).await.unwrap();
    assert!(f.post.is_empty(), "{:?}", f.post);
    assert_eq!(f.scartati, falsi.iter().filter(|e| e["kind"] == 1).count(), "tutti i kind 1 falsi scartati");
}

/// Difesa in profondità: anche se il relay IGNORA il filtro kinds:[1] e manda eventi VALIDI di altri
/// kind (30480 addressable, 20002 effimero, 1059 busta DM), nel feed finiscono SOLO i kind 1 — un DM
/// che comparisse come post sarebbe grave. Prende il sabotaggio `feed-ogni-kind` (toglie `if e.kind == 1`).
#[tokio::test]
async fn eventi_non_post_scartati_anche_se_il_relay_li_manda() {
    let a = chiave(1);
    let eventi = vec![
        firmato(&a, 1, "vero post", 100),
        firmato(&a, 30480, "scheda addressable", 101),
        firmato(&a, 20002, "effimero", 102),
        firmato(&a, 1059, "busta DM", 103),
    ];
    let f = leggi(&Finto::malevolo(eventi), &nodo(), Rete::Prova, BUDGET, |_| {}).await.unwrap();
    assert_eq!(f.post.len(), 1, "solo il kind 1 e un post: {:?}", f.post);
    assert_eq!(f.post[0].testo, "vero post");
    assert!(f.post.iter().all(|p| p.testo != "busta DM"), "un DM (kind 1059) non deve mai comparire nel feed");
    assert_eq!(f.scartati, 3, "i 3 kind != 1 contati come scartati");
}

/// Come nella misura del 20/10: i primi tentativi scadono, poi il descrittore arriva e si riesce.
#[tokio::test(start_paused = true)]
async fn riprova_finche_il_nodo_risponde() {
    let eventi: Vec<Value> = vettori().into_iter().filter(|(ok, _)| *ok).map(|(_, e)| e).collect();
    let fasi = Mutex::new(vec![]);
    let f = leggi(&Finto::new(eventi, 3, false), &nodo(), Rete::Prova, BUDGET, |x| fasi.lock().unwrap().push(x)).await.unwrap();
    assert_eq!(f.tentativi, 4);
    let atteso = (TEMPO_MASSIMO_CONNESSIONE + ATTESA_TRA_TENTATIVI).as_secs() * 3;
    assert!(f.secondi >= atteso && f.secondi <= atteso + 2, "{} s", f.secondi);
    // la fase dice a che tentativo e a che secondo si è: l'interfaccia non resta muta
    let cerco: Vec<_> = fasi.into_inner().unwrap().into_iter().filter_map(|x| match x {
        Fase::CercoIlNodo { secondi, tentativo } => Some((secondi, tentativo)),
        _ => None,
    }).collect();
    assert_eq!(cerco.iter().map(|c| c.1).collect::<Vec<_>>(), vec![1, 2, 3, 4]);
    assert!(cerco.windows(2).all(|w| w[1].0 > w[0].0), "{cerco:?}");
}

#[tokio::test(start_paused = true)]
async fn oltre_il_budget_non_raggiungibile() {
    let fasi = Mutex::new(vec![]);
    let inizio = tokio::time::Instant::now();
    let e = leggi(&Finto::new(vec![], 0, true), &nodo(), Rete::Prova, BUDGET, |x| fasi.lock().unwrap().push(x)).await.unwrap_err();
    assert!(inizio.elapsed() >= BUDGET && inizio.elapsed() < BUDGET + TEMPO_MASSIMO_CONNESSIONE, "{:?}", inizio.elapsed());
    assert!(e.tentativi >= 7, "tentativi: {}", e.tentativi);
    assert!(e.ultimo.contains("tempo massimo"), "{}", e.ultimo);
    assert!(matches!(fasi.into_inner().unwrap().last(), Some(Fase::NonRaggiungibile { .. })));
}

/// Misura del 20/10 (giro 3): budget scaduto, ma tor continua a cercare e la lettura dopo riesce.
/// Qui il trasporto fallisce per 9 tentativi (oltre i 4 minuti) e poi risponde: la ripresa lo prende.
#[tokio::test(start_paused = true)]
async fn dopo_il_budget_attende_un_minuto_e_riprova_da_sola() {
    let eventi: Vec<Value> = vettori().into_iter().filter(|(ok, _)| *ok).map(|(_, e)| e).collect();
    let fasi = Mutex::new(vec![]);
    let f = leggi_con_ripresa(&Finto::new(eventi, 9, false), &nodo(), Rete::Prova, BUDGET, |x| fasi.lock().unwrap().push(x)).await.unwrap();
    assert!(!f.post.is_empty());
    let fasi = fasi.into_inner().unwrap();
    let conto: Vec<u64> = fasi.iter().filter_map(|x| match x { Fase::AttesaRipresa { fra_secondi } => Some(*fra_secondi), _ => None }).collect();
    assert_eq!(conto, (1..=RIPRESA.as_secs()).rev().collect::<Vec<_>>(), "conto alla rovescia di un secondo in un secondo");
    let i_non = fasi.iter().position(|x| matches!(x, Fase::NonRaggiungibile { .. })).expect("prima il budget scade");
    let i_attesa = fasi.iter().position(|x| matches!(x, Fase::AttesaRipresa { .. })).unwrap();
    assert!(i_non < i_attesa, "{fasi:?}");
    assert!(matches!(fasi.last(), Some(Fase::Fatto { .. })), "{:?}", fasi.last());
}

#[tokio::test(start_paused = true)]
async fn la_ripresa_e_una_sola() {
    let finto = Finto::new(vec![], 0, true);
    let inizio = tokio::time::Instant::now();
    let e = leggi_con_ripresa(&finto, &nodo(), Rete::Prova, BUDGET, |_| {}).await.unwrap_err();
    assert!(e.ultimo.contains("tempo massimo"), "{}", e.ultimo);
    let durata = inizio.elapsed();
    assert!(durata >= BUDGET * 2 + RIPRESA && durata < BUDGET * 2 + RIPRESA + TEMPO_MASSIMO_CONNESSIONE * 2, "{durata:?}");
}

// ─── Pubblicazione (F4, post) ────────────────────────────────────────────────

/// Post firmato con l'identità del seme PUBBLICO di prova (lo stesso dei vettori delle chiavi).
fn post(testo: &str) -> m4tr1x_protocol::Evento {
    use m4tr1x_identity::{deriva, seme_da_parole, Ruolo};
    let seme = seme_da_parole(&(["abandon"; 23].join(" ") + " art"), "").unwrap();
    let k = deriva(&seme, Ruolo::Identity, 1).unwrap();
    m4tr1x_protocol::firma_evento(1, testo, &[], 1_760_000_500, Rete::Prova, &k.pubblica_hex(), |d| k.firma_schnorr(d)).unwrap()
}

#[tokio::test]
async fn il_post_pubblicato_si_rilegge_dal_feed() {
    let finto = Finto::new(vec![], 0, false);
    let p = post("primo post dall'app 🦅");
    let fasi = Mutex::new(vec![]);
    let esito = m4tr1x_feed::pubblica(&finto, &nodo(), &p, BUDGET, |x| fasi.lock().unwrap().push(x)).await.unwrap();
    assert_eq!(esito.tentativi, 1);
    let fasi = fasi.into_inner().unwrap();
    assert!(fasi.iter().any(|x| matches!(x, Fase::Invio { .. })), "{fasi:?}");
    assert!(matches!(fasi.last(), Some(Fase::Fatto { .. })), "{fasi:?}");
    // il nodo (finto) l'ha tenuto, e il feed lo rilegge VERIFICATO
    let f = leggi(&finto, &nodo(), Rete::Prova, BUDGET, |_| {}).await.unwrap();
    assert_eq!(f.post.len(), 1);
    assert_eq!(f.post[0].id, p.id);
    assert_eq!(f.post[0].testo, "primo post dall'app 🦅");
}

/// Il "no" del nodo è una risposta: nessun nuovo tentativo, nessuna ripresa.
#[tokio::test(start_paused = true)]
async fn il_no_del_nodo_non_si_ritenta() {
    let finto = Finto::new(vec![], 0, false);
    let inizio = tokio::time::Instant::now();
    let e = m4tr1x_feed::pubblica(&finto, &nodo(), &post(CONTENUTO_RIFIUTATO), BUDGET, |_| {}).await.unwrap_err();
    assert!(matches!(&e, m4tr1x_feed::ErrorePubblica::Rifiutato(m) if m.starts_with("blocked")), "{e:?}");
    assert_eq!(finto.chiamate.load(Ordering::SeqCst), 1, "ritentato dopo un no del nodo");
    assert!(inizio.elapsed() < RIPRESA, "aspettata la ripresa dopo un no del nodo");
}

/// Come il feed: tentativi da 30 s, e la stessa ripresa dopo il budget.
#[tokio::test(start_paused = true)]
async fn pubblica_riprova_e_riprende_come_il_feed() {
    let finto = Finto::new(vec![], 9, false); // oltre il budget: serve la ripresa
    let fasi = Mutex::new(vec![]);
    let p = post("arrivato dopo la ripresa");
    m4tr1x_feed::pubblica(&finto, &nodo(), &p, BUDGET, |x| fasi.lock().unwrap().push(x)).await.unwrap();
    let fasi = fasi.into_inner().unwrap();
    assert!(fasi.iter().any(|x| matches!(x, Fase::AttesaRipresa { .. })), "nessuna ripresa: {fasi:?}");
    assert!(finto.eventi.lock().unwrap().iter().any(|e| e["id"] == p.id.as_str()));
}

// ─── Profili (kind 0) nel feed ───────────────────────────────────────────────

fn chiave(versione: u32) -> m4tr1x_identity::ChiaveRuolo {
    use m4tr1x_identity::{deriva, seme_da_parole, Ruolo};
    let seme = seme_da_parole(&(["abandon"; 23].join(" ") + " art"), "").unwrap();
    deriva(&seme, Ruolo::Identity, versione).unwrap()
}

fn firmato(k: &m4tr1x_identity::ChiaveRuolo, kind: u16, content: &str, quando: u64) -> Value {
    serde_json::to_value(m4tr1x_protocol::firma_evento(kind, content, &[], quando, Rete::Prova, &k.pubblica_hex(), |d| k.firma_schnorr(d)).unwrap()).unwrap()
}

#[tokio::test]
async fn il_feed_mostra_il_nome_dal_profilo_piu_recente() {
    let (a, b) = (chiave(1), chiave(2));
    let mut falso = firmato(&b, 0, r#"{"name":"Impostore"}"#, 300);
    falso["pubkey"] = Value::String(a.pubblica_hex()); // profilo con la chiave di A ma firmato da B
    let eventi = vec![
        firmato(&a, 1, "post di A", 100),
        firmato(&b, 1, "post di B", 101),
        firmato(&a, 0, r#"{"name":"Vecchio nome"}"#, 50),
        firmato(&a, 0, r#"{"name":"Arta","about":"x","lud16":"paga@esempio"}"#, 60),
        falso,
    ];
    let f = leggi(&Finto::new(eventi, 0, false), &nodo(), Rete::Prova, BUDGET, |_| {}).await.unwrap();
    let nome = |testo: &str| f.post.iter().find(|p| p.testo == testo).unwrap().nome.clone();
    assert_eq!(nome("post di A"), Some("Arta".into()), "vince il profilo più recente e VALIDO");
    assert_eq!(nome("post di B"), None, "B non ha un profilo: nessun nome inventato");
}

#[tokio::test]
async fn leggi_profilo_per_precompilare() {
    let a = chiave(1);
    let eventi = vec![firmato(&a, 0, r#"{"name":"Arta","about":"ciao"}"#, 10), firmato(&a, 0, r#"{"name":"Vecchio"}"#, 5)];
    let finto = Finto::new(eventi, 0, false);
    let p = m4tr1x_feed::leggi_profilo(&finto, &nodo(), Rete::Prova, &a.pubblica_hex(), BUDGET, |_| {}).await.unwrap();
    assert_eq!(p, Some(m4tr1x_protocol::Profilo { name: "Arta".into(), about: "ciao".into() }));
    let nessuno = m4tr1x_feed::leggi_profilo(&finto, &nodo(), Rete::Prova, &chiave(2).pubblica_hex(), BUDGET, |_| {}).await.unwrap();
    assert_eq!(nessuno, None);
    // anche se il relay IGNORA authors:[B] e manda il profilo di A, leggi_profilo(B) non lo restituisce
    let mal = Finto::malevolo(vec![firmato(&a, 0, r#"{"name":"Arta"}"#, 10)]);
    let per_b = m4tr1x_feed::leggi_profilo(&mal, &nodo(), Rete::Prova, &chiave(2).pubblica_hex(), BUDGET, |_| {}).await.unwrap();
    assert_eq!(per_b, None, "il profilo di un altro autore non torna per B anche se il relay lo manda");
}

#[tokio::test]
async fn profilo_salvato_si_rilegge_e_da_il_nome() {
    let a = chiave(1);
    let finto = Finto::new(vec![firmato(&a, 1, "un post", 100)], 0, false);
    let c = m4tr1x_protocol::contenuto_profilo("Arta", "prova").unwrap();
    let ev = m4tr1x_protocol::firma_evento(0, &c, &[], 200, Rete::Prova, &a.pubblica_hex(), |d| a.firma_schnorr(d)).unwrap();
    m4tr1x_feed::pubblica(&finto, &nodo(), &ev, BUDGET, |_| {}).await.unwrap();
    let f = leggi(&finto, &nodo(), Rete::Prova, BUDGET, |_| {}).await.unwrap();
    assert_eq!(f.post[0].nome.as_deref(), Some("Arta"));
}

/// Difesa in profondità sul giro dei profili (kinds:[0]) con un relay che manda tutto: il nome di un post
/// arriva SOLO da un profilo kind 0 dell'autore. Caso POSITIVO incluso (il profilo vero di A, in mezzo alla
/// spazzatura, va ACCETTATO: così un codice che rifiuta tutto non passerebbe). L'impostore è un kind 1
/// "travestito" da profilo e più recente: a codice intatto viene scartato, con la mutazione del kind no.
#[tokio::test]
async fn il_nome_solo_da_un_profilo_kind0_dell_autore() {
    let (a, b) = (chiave(1), chiave(2));
    let eventi = vec![
        firmato(&a, 1, "post di A", 100),                  // il post
        firmato(&a, 0, r#"{"name":"Arta"}"#, 202),          // profilo VERO di A: va ACCETTATO (caso positivo)
        firmato(&a, 1, r#"{"name":"Impostore"}"#, 203),     // kind 1 travestito da profilo, più recente
        firmato(&b, 0, r#"{"name":"Altro"}"#, 204),         // profilo di B (altro autore): non si attacca ad A
    ];
    let f = leggi(&Finto::malevolo(eventi), &nodo(), Rete::Prova, BUDGET, |_| {}).await.unwrap();
    let pa = f.post.iter().find(|p| p.testo == "post di A").expect("manca il post di A");
    assert_eq!(pa.nome.as_deref(), Some("Arta"), "il profilo VERO (kind 0) dell'autore va accettato, l'impostore kind 1 no");
}
