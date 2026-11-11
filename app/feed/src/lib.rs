//! Feed dell'app (F4, passo 4; decisioni del fondatore del 20/10/2026): gli ULTIMI POST del nodo di
//! prova, presi via Tor e verificati nel lato Rust prima di arrivare all'interfaccia.
//!
//! Tentativi: ognuno dura al massimo `TEMPO_MASSIMO_CONNESSIONE` (30 s, nel trasporto); si riprova
//! fino a un BUDGET di 4 minuti. Misura del 20/10: il primo collegamento a un onion può richiedere da
//! 2 s a 251 s, perché tor aspetta fino a ~120 s gli HSDir che non rispondono; tagliare un tentativo
//! non accorcia la ricerca, ma appena il descrittore arriva il tentativo dopo riesce in meno di 1 s.
//! Durante l'attesa si segnala la FASE con i secondi passati: l'app non deve sembrare bloccata.
//!
//! Ogni evento passa da `m4tr1x_protocol::verifica` (forma, rete, dati H8, id, firma): quelli non
//! validi, o non kind 1, si scartano e si contano. Nessun indirizzo onion esce da qui verso l'interfaccia.

use m4tr1x_protocol::{verifica, Rete};
use m4tr1x_transport::{Isolation, OnionV3, Relay, Transport, TransportError};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::Instant;

/// Tempo totale per raggiungere il nodo e scaricare il feed (peggiore misurato il 20/10: 251 s).
pub const BUDGET: Duration = Duration::from_secs(240);
/// Pausa tra un tentativo e il successivo.
pub const ATTESA_TRA_TENTATIVI: Duration = Duration::from_secs(2);
/// Quanti post si chiedono al nodo.
pub const QUANTI: u64 = 50;
/// Circuito del feed, separato da quelli delle altre attività dell'app (I-37, IsolateSOCKSAuth).
pub const ISOLAMENTO_FEED: Isolation = Isolation(0xFEED);

/// Dopo un budget scaduto si riprova da soli una volta, dopo questa attesa (fondatore, 20/10/2026:
/// tor continua a cercare il descrittore anche dopo; nelle misure la lettura dopo è riuscita in 19–56 s).
pub const RIPRESA: Duration = Duration::from_secs(60);

/// Dove si trova il feed, per l'interfaccia ("sto cercando il nodo… 47 s").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fase {
    CercoIlNodo { secondi: u64, tentativo: u32 },
    Scarico { secondi: u64, tentativo: u32 },
    /// collegati al nodo, il post firmato è partito: si aspetta la conferma (`OK`)
    Invio { secondi: u64, tentativo: u32 },
    Fatto { secondi: u64, tentativo: u32 },
    NonRaggiungibile { secondi: u64, tentativo: u32 },
    /// budget scaduto: "il nodo non risponde ancora, Tor continua a cercarlo: riprovo tra N s"
    AttesaRipresa { fra_secondi: u64 },
}

/// Un post verificato.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Post {
    pub id: String,
    pub autore: String,
    pub creato: u64,
    pub testo: String,
    /// nome dal profilo (kind 0) dell'autore, già pulito (m4tr1x_protocol::profilo_da_evento); None se non ne ha
    pub nome: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feed {
    /// dal più recente
    pub post: Vec<Post>,
    /// eventi arrivati ma non validi (firma, rete, dati H8…) o non post (kind diverso da 1)
    pub scartati: usize,
    pub secondi: u64,
    pub tentativi: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub struct NonRaggiungibile {
    pub secondi: u64,
    pub tentativi: u32,
    /// ultimo errore del trasporto (es. "SOCKS ha risposto 240 (descrittore dell'onion non trovato)")
    pub ultimo: String,
}

/// Il ciclo dei tentativi, comune a lettura e pubblicazione: `un_tentativo(n, inizio)` fino al budget,
/// con la fase "cerco il nodo" a ogni tentativo. Restituisce (risultato, secondi, tentativi).
async fn con_tentativi<R, Fut>(budget: Duration, avanza: &impl Fn(Fase), mut un_tentativo: impl FnMut(u32, Instant) -> Fut) -> Result<(R, u64, u32), NonRaggiungibile>
where
    Fut: std::future::Future<Output = Result<R, TransportError>>,
{
    let t0 = Instant::now();
    let secondi = || t0.elapsed().as_secs();
    let mut ultimo = String::from("nessun tentativo");
    let mut tentativo = 0;
    loop {
        let resto = budget.saturating_sub(t0.elapsed());
        if resto.is_zero() {
            avanza(Fase::NonRaggiungibile { secondi: secondi(), tentativo });
            return Err(NonRaggiungibile { secondi: secondi(), tentativi: tentativo, ultimo });
        }
        tentativo += 1;
        avanza(Fase::CercoIlNodo { secondi: secondi(), tentativo });
        match tokio::time::timeout(resto, un_tentativo(tentativo, t0)).await {
            Ok(Ok(r)) => return Ok((r, secondi(), tentativo)),
            Ok(Err(e)) => ultimo = e.to_string(),
            // la causa vera dell'ultimo errore resta (es. "240: descrittore non trovato"): si aggiunge, non si sostituisce
            Err(_) => ultimo = format!("{ultimo} · poi budget scaduto durante il tentativo {tentativo}"),
        }
        tokio::time::sleep(ATTESA_TRA_TENTATIVI.min(budget.saturating_sub(t0.elapsed()))).await;
    }
}

/// Se `operazione` non arriva al nodo entro il budget, aspetta [`RIPRESA`] con il conto alla rovescia
/// ([`Fase::AttesaRipresa`], ogni secondo) e la rifà UNA volta (fondatore, 20/10/2026).
async fn con_ripresa<R, Fut>(avanza: &impl Fn(Fase), mut operazione: impl FnMut() -> Fut) -> Result<R, NonRaggiungibile>
where
    Fut: std::future::Future<Output = Result<R, NonRaggiungibile>>,
{
    if let Ok(r) = operazione().await {
        return Ok(r);
    }
    for mancano in (1..=RIPRESA.as_secs()).rev() {
        avanza(Fase::AttesaRipresa { fra_secondi: mancano });
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    operazione().await
}

/// Legge il feed dal nodo entro `budget`, chiamando `avanza` a ogni cambio di fase.
pub async fn leggi<T: Transport>(t: &T, nodo: &OnionV3, rete: Rete, budget: Duration, avanza: impl Fn(Fase)) -> Result<Feed, NonRaggiungibile> {
    let avanza = &avanza;
    let ((post, scartati), secondi, tentativi) =
        con_tentativi(budget, avanza, |tentativo, t0| un_tentativo(t, nodo, rete, move || avanza(Fase::Scarico { secondi: t0.elapsed().as_secs(), tentativo })))
            .await?;
    avanza(Fase::Fatto { secondi, tentativo: tentativi });
    Ok(Feed { post, scartati, secondi, tentativi })
}

/// Come [`leggi`], ma con la ripresa automatica dopo il budget.
pub async fn leggi_con_ripresa<T: Transport>(t: &T, nodo: &OnionV3, rete: Rete, budget: Duration, avanza: impl Fn(Fase)) -> Result<Feed, NonRaggiungibile> {
    con_ripresa(&avanza, || leggi(t, nodo, rete, budget, &avanza)).await
}

/// Post pubblicato: quanto ci è voluto.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pubblicato {
    pub secondi: u64,
    pub tentativi: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ErrorePubblica {
    /// il nodo ha risposto NO (`OK` false): non si ritenta, è una risposta
    Rifiutato(String),
    NonRaggiungibile(NonRaggiungibile),
}

/// Pubblica un evento GIÀ firmato e verificato (m4tr1x_protocol::firma_evento) sul nodo, con i tentativi
/// e la ripresa del feed. Ripubblicare lo stesso evento è innocuo (il nodo risponde "duplicate").
pub async fn pubblica<T: Transport>(t: &T, nodo: &OnionV3, evento: &m4tr1x_protocol::Evento, budget: Duration, avanza: impl Fn(Fase)) -> Result<Pubblicato, ErrorePubblica> {
    let json = serde_json::to_value(evento).expect("un evento si serializza sempre");
    let (json, avanza) = (&json, &avanza);
    let una_volta = || async move {
        con_tentativi(budget, avanza, |tentativo, t0| async move {
            let flusso = t.connect_onion(nodo, 80, ISOLAMENTO_FEED).await?;
            let mut relay = Relay::apri(flusso, nodo).await?;
            avanza(Fase::Invio { secondi: t0.elapsed().as_secs(), tentativo });
            let r = relay.pubblica(json).await;
            relay.chiudi().await;
            r
        })
        .await
    };
    let ((accettato, motivo), secondi, tentativi) = con_ripresa(avanza, una_volta).await.map_err(ErrorePubblica::NonRaggiungibile)?;
    if !accettato {
        return Err(ErrorePubblica::Rifiutato(motivo));
    }
    avanza(Fase::Fatto { secondi, tentativo: tentativi });
    Ok(Pubblicato { secondi, tentativi })
}

/// Più richieste nella STESSA connessione al nodo (una risposta per filtro, nell'ordine), con i tentativi
/// e la ripresa del feed. Gli eventi tornano GREZZI: chi chiama li deve verificare uno per uno prima di
/// crederci (la posta lo fa con `m4tr1x-dm`). Se una richiesta fallisce, fallisce il tentativo intero.
pub async fn richiedi<T: Transport>(t: &T, nodo: &OnionV3, filtri: &[serde_json::Value], budget: Duration, avanza: impl Fn(Fase)) -> Result<Vec<Vec<serde_json::Value>>, NonRaggiungibile> {
    let avanza = &avanza;
    let una_volta = || async move {
        con_tentativi(budget, avanza, |tentativo, t0| async move {
            let flusso = t.connect_onion(nodo, 80, ISOLAMENTO_FEED).await?;
            let mut relay = Relay::apri(flusso, nodo).await?;
            avanza(Fase::Scarico { secondi: t0.elapsed().as_secs(), tentativo });
            let mut risposte = Vec::with_capacity(filtri.len());
            for f in filtri {
                match relay.richiedi(f.clone()).await {
                    Ok(r) => risposte.push(r),
                    Err(e) => {
                        relay.chiudi().await;
                        return Err(e);
                    }
                }
            }
            relay.chiudi().await;
            Ok(risposte)
        })
        .await
    };
    let (risposte, secondi, tentativi) = con_ripresa(avanza, una_volta).await?;
    avanza(Fase::Fatto { secondi, tentativo: tentativi });
    Ok(risposte)
}

async fn un_tentativo<T: Transport>(t: &T, nodo: &OnionV3, rete: Rete, scarico: impl Fn()) -> Result<(Vec<Post>, usize), TransportError> {
    // ogni collegamento dura al massimo TEMPO_MASSIMO_CONNESSIONE (30 s): lo garantisce il trasporto
    let flusso = t.connect_onion(nodo, 80, ISOLAMENTO_FEED).await?;
    let mut relay = Relay::apri(flusso, nodo).await?;
    scarico();
    let grezzi = match relay.richiedi(json!({ "kinds": [1], "limit": QUANTI })).await {
        Ok(g) => g,
        Err(e) => {
            relay.chiudi().await;
            return Err(e);
        }
    };
    let (mut post, scartati) = verificati(grezzi, rete);
    // nella stessa connessione: i profili (kind 0) degli autori, per mostrare il nome
    let autori: Vec<&str> = post.iter().map(|p| p.autore.as_str()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    if !autori.is_empty() {
        let profili = relay.richiedi(json!({ "kinds": [0], "authors": autori, "limit": QUANTI })).await;
        let nomi = nomi_dai_profili(profili.unwrap_or_default(), rete);
        for p in &mut post {
            p.nome = nomi.get(&p.autore).cloned();
        }
    }
    relay.chiudi().await;
    Ok((post, scartati))
}

/// Nome per autore dai profili (kind 0): solo eventi VERIFICATI, vince il più recente di ciascun autore,
/// profili non leggibili o con nome vuoto ignorati.
pub fn nomi_dai_profili(grezzi: Vec<serde_json::Value>, rete: Rete) -> std::collections::HashMap<String, String> {
    let mut piu_recenti: std::collections::HashMap<String, (u64, String)> = Default::default();
    for g in grezzi {
        let Ok(e) = verifica(&g, rete) else { continue };
        let Some(p) = m4tr1x_protocol::profilo_da_evento(&e) else { continue };
        if p.name.is_empty() {
            continue;
        }
        if piu_recenti.get(&e.pubkey).is_none_or(|(t, _)| e.created_at > *t) {
            piu_recenti.insert(e.pubkey, (e.created_at, p.name));
        }
    }
    piu_recenti.into_iter().map(|(k, (_, n))| (k, n)).collect()
}

/// Profilo di un autore dal nodo (es. il tuo, per precompilare il modulo), con i tentativi e la ripresa del
/// feed. Ok(None) se il nodo non ne ha uno valido.
pub async fn leggi_profilo<T: Transport>(t: &T, nodo: &OnionV3, rete: Rete, autore: &str, budget: Duration, avanza: impl Fn(Fase)) -> Result<Option<m4tr1x_protocol::Profilo>, NonRaggiungibile> {
    let avanza = &avanza;
    let filtro = json!({ "kinds": [0], "authors": [autore], "limit": 5 });
    let filtro = &filtro;
    let una_volta = || async move {
        con_tentativi(budget, avanza, |tentativo, t0| async move {
            let flusso = t.connect_onion(nodo, 80, ISOLAMENTO_FEED).await?;
            let mut relay = Relay::apri(flusso, nodo).await?;
            avanza(Fase::Scarico { secondi: t0.elapsed().as_secs(), tentativo });
            let r = relay.richiedi(filtro.clone()).await;
            relay.chiudi().await;
            r
        })
        .await
    };
    let (grezzi, secondi, tentativi) = con_ripresa(avanza, una_volta).await?;
    avanza(Fase::Fatto { secondi, tentativo: tentativi });
    let piu_recente = grezzi
        .iter()
        .filter_map(|g| verifica(g, rete).ok())
        .filter(|e| e.pubkey == autore)
        .max_by_key(|e| e.created_at);
    Ok(piu_recente.as_ref().and_then(m4tr1x_protocol::profilo_da_evento))
}

/// Solo eventi validi e kind 1, senza doppioni, dal più recente.
pub fn verificati(grezzi: Vec<serde_json::Value>, rete: Rete) -> (Vec<Post>, usize) {
    let mut scartati = 0;
    let mut post: Vec<Post> = vec![];
    for g in grezzi {
        match verifica(&g, rete) {
            // discussioni e risposte del forum: valide, ma non sono post di Signal (né scartate)
            Ok(e) if e.kind == 1 && m4tr1x_protocol::e_del_forum(&e) => {}
            Ok(e) if e.kind == 1 => {
                if !post.iter().any(|p| p.id == e.id) {
                    post.push(Post { id: e.id, autore: e.pubkey, creato: e.created_at, testo: e.content, nome: None });
                }
            }
            _ => scartati += 1,
        }
    }
    post.sort_by(|a, b| b.creato.cmp(&a.creato).then_with(|| a.id.cmp(&b.id)));
    (post, scartati)
}

// ─── Forum ───────────────────────────────────────────────────────────────────
// Stesso schema della v2 (m4tr1x_protocol: TAG_FORUM, discussione_da_evento, risposta_a). Ogni evento passa da
// `verifica`; i nomi vengono SOLO da profili verificati, come nel feed.

/// Quante discussioni si chiedono al nodo, e quante risposte al massimo (il client del relay ne accetta 500).
pub const QUANTE_DISCUSSIONI: u64 = 100;
pub const QUANTE_RISPOSTE: u64 = 500;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discussione {
    pub id: String,
    pub autore: String,
    pub nome: Option<String>,
    pub titolo: String,
    pub testo: String,
    pub creato: u64,
    /// risposte valide trovate sul nodo
    pub risposte: u32,
    /// ora dell'ultima risposta (o della discussione): l'elenco si ordina da qui
    pub attivita: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Forum {
    pub discussioni: Vec<Discussione>,
    pub scartati: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscussioneAperta {
    pub discussione: Discussione,
    /// dalla più vecchia
    pub risposte: Vec<Post>,
}

fn verificato(g: &Value, rete: Rete) -> Option<m4tr1x_protocol::Evento> {
    verifica(g, rete).ok()
}

/// Le discussioni del nodo, dalla più attiva, con il numero di risposte e i nomi degli autori.
pub async fn leggi_forum<T: Transport>(t: &T, nodo: &OnionV3, rete: Rete, budget: Duration, avanza: impl Fn(Fase)) -> Result<Forum, NonRaggiungibile> {
    let avanza = &avanza;
    let primo = richiedi(t, nodo, &[json!({ "kinds": [1], "#t": [m4tr1x_protocol::TAG_FORUM], "limit": QUANTE_DISCUSSIONI })], budget, avanza).await?;
    let mut scartati = 0;
    let mut discussioni: Vec<Discussione> = vec![];
    for g in primo.into_iter().flatten() {
        let Some(e) = verificato(&g, rete) else {
            scartati += 1;
            continue;
        };
        if let Some(titolo) = m4tr1x_protocol::discussione_da_evento(&e) {
            if !discussioni.iter().any(|d| d.id == e.id) {
                discussioni.push(Discussione { attivita: e.created_at, id: e.id, autore: e.pubkey, nome: None, titolo, testo: e.content, creato: e.created_at, risposte: 0 });
            }
        }
    }
    if !discussioni.is_empty() {
        let ids: Vec<&str> = discussioni.iter().map(|d| d.id.as_str()).collect();
        let mut autori: Vec<&str> = discussioni.iter().map(|d| d.autore.as_str()).collect();
        autori.sort_unstable();
        autori.dedup();
        let filtri = [json!({ "kinds": [1], "#e": ids, "limit": QUANTE_RISPOSTE }), json!({ "kinds": [0], "authors": autori, "limit": QUANTE_DISCUSSIONI })];
        // risposte e nomi sono un di più: senza, l'elenco si mostra lo stesso
        if let Ok(mut r) = richiedi(t, nodo, &filtri, budget, avanza).await {
            let nomi = nomi_dai_profili(r.pop().unwrap_or_default(), rete);
            let mut viste = std::collections::HashSet::new();
            for g in r.pop().unwrap_or_default() {
                let Some(e) = verificato(&g, rete) else { continue };
                let Some(radice) = m4tr1x_protocol::risposta_a(&e) else { continue };
                if !viste.insert(e.id.clone()) {
                    continue;
                }
                if let Some(d) = discussioni.iter_mut().find(|d| d.id == radice) {
                    d.risposte += 1;
                    d.attivita = d.attivita.max(e.created_at);
                }
            }
            for d in &mut discussioni {
                d.nome = nomi.get(&d.autore).cloned();
            }
        }
    }
    discussioni.sort_by(|a, b| b.attivita.cmp(&a.attivita).then_with(|| a.id.cmp(&b.id)));
    Ok(Forum { discussioni, scartati })
}

/// Una discussione con tutte le sue risposte e i nomi di chi scrive. Ok(None): il nodo non ce l'ha.
pub async fn leggi_discussione<T: Transport>(t: &T, nodo: &OnionV3, rete: Rete, id: &str, budget: Duration, avanza: impl Fn(Fase)) -> Result<Option<DiscussioneAperta>, NonRaggiungibile> {
    let avanza = &avanza;
    let filtri = [json!({ "ids": [id], "limit": 1 }), json!({ "kinds": [1], "#e": [id], "limit": QUANTE_RISPOSTE })];
    let mut r = richiedi(t, nodo, &filtri, budget, avanza).await?;
    let grezze = r.pop().unwrap_or_default();
    let radice = r.pop().unwrap_or_default().iter().filter_map(|g| verificato(g, rete)).find(|e| e.id == id);
    let Some(e) = radice else { return Ok(None) };
    let Some(titolo) = m4tr1x_protocol::discussione_da_evento(&e) else { return Ok(None) };
    let mut risposte: Vec<Post> = vec![];
    for g in grezze {
        let Some(r) = verificato(&g, rete) else { continue };
        if m4tr1x_protocol::risposta_a(&r).as_deref() == Some(id) && !risposte.iter().any(|p| p.id == r.id) {
            risposte.push(Post { id: r.id, autore: r.pubkey, creato: r.created_at, testo: r.content, nome: None });
        }
    }
    risposte.sort_by(|a, b| a.creato.cmp(&b.creato).then_with(|| a.id.cmp(&b.id)));
    let mut d = Discussione { attivita: risposte.last().map_or(e.created_at, |p| p.creato.max(e.created_at)), risposte: risposte.len() as u32, id: e.id, autore: e.pubkey, nome: None, titolo, testo: e.content, creato: e.created_at };
    let mut autori: Vec<&str> = std::iter::once(d.autore.as_str()).chain(risposte.iter().map(|p| p.autore.as_str())).collect();
    autori.sort_unstable();
    autori.dedup();
    if let Ok(mut p) = richiedi(t, nodo, &[json!({ "kinds": [0], "authors": autori, "limit": QUANTE_DISCUSSIONI })], budget, avanza).await {
        let nomi = nomi_dai_profili(p.pop().unwrap_or_default(), rete);
        d.nome = nomi.get(&d.autore).cloned();
        for r in &mut risposte {
            r.nome = nomi.get(&r.autore).cloned();
        }
    }
    Ok(Some(DiscussioneAperta { discussione: d, risposte }))
}

// ─── File: reels, film, musica ───────────────────────────────────────────────
// Le schede (kind 1063) passano da `verifica` e da `file_da_evento`; il file si scarica per hash e si accetta
// solo se l'hash torna (m4tr1x_transport::http). Nomi solo da profili verificati.

/// Quante schede si chiedono per sezione.
pub const QUANTI_FILE: u64 = 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Media {
    pub id: String,
    pub autore: String,
    pub nome: Option<String>,
    /// didascalia
    pub testo: String,
    pub creato: u64,
    pub file: m4tr1x_protocol::SchedaFile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Galleria {
    pub media: Vec<Media>,
    pub scartati: usize,
}

/// Le schede di una sezione, dalla più recente, con i nomi degli autori.
pub async fn leggi_media<T: Transport>(t: &T, nodo: &OnionV3, rete: Rete, sezione: m4tr1x_protocol::Sezione, budget: Duration, avanza: impl Fn(Fase)) -> Result<Galleria, NonRaggiungibile> {
    let avanza = &avanza;
    let filtro = json!({ "kinds": [m4tr1x_protocol::KIND_FILE], "#t": [sezione.tag()], "limit": QUANTI_FILE });
    let grezzi = richiedi(t, nodo, &[filtro], budget, avanza).await?.into_iter().flatten();
    let mut scartati = 0;
    let mut media: Vec<Media> = vec![];
    for g in grezzi {
        let scheda = verificato(&g, rete).and_then(|e| m4tr1x_protocol::file_da_evento(&e).filter(|f| f.sezione == sezione).map(|f| (e, f)));
        let Some((e, file)) = scheda else {
            scartati += 1;
            continue;
        };
        if !media.iter().any(|m| m.id == e.id) {
            media.push(Media { id: e.id, autore: e.pubkey, nome: None, testo: e.content, creato: e.created_at, file });
        }
    }
    if !media.is_empty() {
        let mut autori: Vec<&str> = media.iter().map(|m| m.autore.as_str()).collect();
        autori.sort_unstable();
        autori.dedup();
        if let Ok(mut p) = richiedi(t, nodo, &[json!({ "kinds": [0], "authors": autori, "limit": QUANTI_FILE })], budget, avanza).await {
            let nomi = nomi_dai_profili(p.pop().unwrap_or_default(), rete);
            for m in &mut media {
                m.nome = nomi.get(&m.autore).cloned();
            }
        }
    }
    media.sort_by(|a, b| b.creato.cmp(&a.creato).then_with(|| a.id.cmp(&b.id)));
    Ok(Galleria { media, scartati })
}

/// Scarica il file `sha256` (al massimo `dimensione` byte, come dice la scheda): solo se l'hash torna (I-17).
pub async fn scarica_file<T: Transport>(t: &T, nodo: &OnionV3, sha256: &str, dimensione: usize, avanza: impl Fn(Fase)) -> Result<Vec<u8>, NonRaggiungibile> {
    let budget = BUDGET + m4tr1x_transport::http::tempo_per(dimensione);
    let avanza = &avanza;
    let una_volta = || async move {
        con_tentativi(budget, avanza, |tentativo, t0| async move {
            let flusso = t.connect_onion(nodo, 80, ISOLAMENTO_FEED).await?;
            avanza(Fase::Scarico { secondi: t0.elapsed().as_secs(), tentativo });
            m4tr1x_transport::http::scarica(flusso, nodo, sha256, dimensione).await
        })
        .await
    };
    let (b, secondi, tentativi) = con_ripresa(avanza, una_volta).await?;
    avanza(Fase::Fatto { secondi, tentativo: tentativi });
    Ok(b)
}

/// Carica un file GIÀ pulito con l'autorizzazione GIÀ firmata. Un "no" del nodo non si ritenta: è una risposta.
pub async fn carica_file<T: Transport>(t: &T, nodo: &OnionV3, autorizzazione: &str, corpo: &[u8], avanza: impl Fn(Fase)) -> Result<m4tr1x_transport::http::Caricato, NonRaggiungibile> {
    let budget = BUDGET + m4tr1x_transport::http::tempo_per(corpo.len());
    let avanza = &avanza;
    let una_volta = || async move {
        con_tentativi(budget, avanza, |tentativo, t0| async move {
            let flusso = t.connect_onion(nodo, 80, ISOLAMENTO_FEED).await?;
            avanza(Fase::Invio { secondi: t0.elapsed().as_secs(), tentativo });
            m4tr1x_transport::http::carica(flusso, nodo, autorizzazione, corpo).await
        })
        .await
    };
    let (r, secondi, tentativi) = con_ripresa(avanza, una_volta).await?;
    avanza(Fase::Fatto { secondi, tentativo: tentativi });
    Ok(r)
}
