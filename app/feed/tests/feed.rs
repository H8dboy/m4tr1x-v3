//! Feed con un trasporto finto (niente rete, niente Tor, tempo virtuale): eventi verificati, tentativi
//! fino al budget, fase segnalata. Gli eventi sono i vettori CONGELATI di packages/protocol.

use m4tr1x_feed::{leggi, leggi_con_ripresa, Fase, ATTESA_TRA_TENTATIVI, BUDGET, RIPRESA};
use m4tr1x_protocol::Rete;
use m4tr1x_transport::prova::relay_finto;
use m4tr1x_transport::{BootstrapInfo, Isolation, OnionV3, Stream, Transport, TransportError, TransportStatus, TEMPO_MASSIMO_CONNESSIONE};
use serde_json::Value;
use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

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
/// non riesce mai; poi un relay finto che manda `eventi`.
struct Finto {
    eventi: Vec<Value>,
    falliscono: u32,
    sempre_appeso: bool,
    chiamate: AtomicU32,
}

impl Finto {
    fn new(eventi: Vec<Value>, falliscono: u32, sempre_appeso: bool) -> Self {
        Self { eventi, falliscono, sempre_appeso, chiamate: AtomicU32::new(0) }
    }
}

impl Transport for Finto {
    fn bootstrap(&self) -> impl Future<Output = m4tr1x_transport::Result<BootstrapInfo>> + Send {
        async { Ok(BootstrapInfo { millisecondi: 0, implementazione: "finto" }) }
    }
    fn connect_onion(&self, _h: &OnionV3, _p: u16, _i: Isolation) -> impl Future<Output = m4tr1x_transport::Result<Stream>> + Send {
        let n = self.chiamate.fetch_add(1, Ordering::SeqCst) + 1;
        let (eventi, falliscono, appeso) = (self.eventi.clone(), self.falliscono, self.sempre_appeso);
        async move {
            if appeso || n <= falliscono {
                tokio::time::sleep(TEMPO_MASSIMO_CONNESSIONE).await;
                return Err(TransportError::Connessione("tempo massimo del tentativo superato (30 s)".into()));
            }
            let (client, server) = tokio::io::duplex(1 << 20);
            tokio::spawn(relay_finto(server, eventi));
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
    assert_eq!(f.scartati, v.len() - attesi, "non validi o non kind 1");
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
    assert_eq!(f.scartati, falsi.len());
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
