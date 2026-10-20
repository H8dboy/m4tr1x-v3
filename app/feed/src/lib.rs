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
use serde_json::json;
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

/// Legge il feed dal nodo entro `budget`, chiamando `avanza` a ogni cambio di fase.
pub async fn leggi<T: Transport>(t: &T, nodo: &OnionV3, rete: Rete, budget: Duration, avanza: impl Fn(Fase)) -> Result<Feed, NonRaggiungibile> {
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
        match tokio::time::timeout(resto, un_tentativo(t, nodo, rete, || avanza(Fase::Scarico { secondi: secondi(), tentativo }))).await {
            Ok(Ok((post, scartati))) => {
                avanza(Fase::Fatto { secondi: secondi(), tentativo });
                return Ok(Feed { post, scartati, secondi: secondi(), tentativi: tentativo });
            }
            Ok(Err(e)) => ultimo = e.to_string(),
            // la causa vera dell'ultimo errore resta (es. "240: descrittore non trovato"): si aggiunge, non si sostituisce
            Err(_) => ultimo = format!("{ultimo} · poi budget scaduto durante il tentativo {tentativo}"),
        }
        tokio::time::sleep(ATTESA_TRA_TENTATIVI.min(budget.saturating_sub(t0.elapsed()))).await;
    }
}

/// Come [`leggi`], ma se il budget scade aspetta [`RIPRESA`] mostrando il conto alla rovescia
/// ([`Fase::AttesaRipresa`], ogni secondo) e rilegge UNA volta. L'errore finale è quello della ripresa.
pub async fn leggi_con_ripresa<T: Transport>(t: &T, nodo: &OnionV3, rete: Rete, budget: Duration, avanza: impl Fn(Fase)) -> Result<Feed, NonRaggiungibile> {
    if let Ok(f) = leggi(t, nodo, rete, budget, &avanza).await {
        return Ok(f);
    }
    for mancano in (1..=RIPRESA.as_secs()).rev() {
        avanza(Fase::AttesaRipresa { fra_secondi: mancano });
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    leggi(t, nodo, rete, budget, &avanza).await
}

async fn un_tentativo<T: Transport>(t: &T, nodo: &OnionV3, rete: Rete, scarico: impl Fn()) -> Result<(Vec<Post>, usize), TransportError> {
    // ogni collegamento dura al massimo TEMPO_MASSIMO_CONNESSIONE (30 s): lo garantisce il trasporto
    let flusso = t.connect_onion(nodo, 80, ISOLAMENTO_FEED).await?;
    let mut relay = Relay::apri(flusso, nodo).await?;
    scarico();
    let grezzi = relay.richiedi(json!({ "kinds": [1], "limit": QUANTI })).await;
    relay.chiudi().await;
    Ok(verificati(grezzi?, rete))
}

/// Solo eventi validi e kind 1, senza doppioni, dal più recente.
pub fn verificati(grezzi: Vec<serde_json::Value>, rete: Rete) -> (Vec<Post>, usize) {
    let mut scartati = 0;
    let mut post: Vec<Post> = vec![];
    for g in grezzi {
        match verifica(&g, rete) {
            Ok(e) if e.kind == 1 => {
                if !post.iter().any(|p| p.id == e.id) {
                    post.push(Post { id: e.id, autore: e.pubkey, creato: e.created_at, testo: e.content });
                }
            }
            _ => scartati += 1,
        }
    }
    post.sort_by(|a, b| b.creato.cmp(&a.creato).then_with(|| a.id.cmp(&b.id)));
    (post, scartati)
}
