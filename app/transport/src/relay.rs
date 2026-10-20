//! Client del relay del nodo (WebSocket su `/relay`, protocollo Nostr NIP-01) sopra un flusso
//! onion GIÀ aperto da [`crate::Transport::connect_onion`]. Qui non si apre nessun socket:
//! `tokio-tungstenite` è compilato senza `connect` e senza TLS, fa solo la stretta di mano
//! WebSocket sul flusso che riceve (I-36, ADR-0016; decisione del fondatore, 20/10/2026).
//!
//! Gli eventi escono di qui come JSON grezzo: li verifica il chiamante (`m4tr1x-protocol`)
//! prima di mostrarli. Limiti contro un nodo che risponde troppo o non risponde:
//! messaggio ≤ 512 KiB, ≤ [`MASSIMO_EVENTI`] eventi per richiesta, [`TEMPO_MASSIMO`] per richiesta.

use crate::{OnionV3, Result, Stream, TransportError};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::{timeout_at, Instant};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// Messaggio più grande accettato dal nodo (stesso limite dei peer del nodo, `maxPayload`).
pub const MASSIMO_MESSAGGIO: usize = 512 * 1024;
/// Eventi accettati per una richiesta: oltre, la richiesta si chiude con errore.
pub const MASSIMO_EVENTI: usize = 500;
/// Tempo massimo di una richiesta (o dell'apertura) via Tor.
pub const TEMPO_MASSIMO: Duration = Duration::from_secs(60);

fn errore(e: impl std::fmt::Display) -> TransportError {
    TransportError::Relay(e.to_string())
}

pub struct Relay {
    ws: WebSocketStream<Stream>,
    prossimo: u64,
}

impl Relay {
    /// Stretta di mano WebSocket su `ws://<onion>/relay` (l'header Host è l'onion: il nodo lo controlla).
    pub async fn apri(flusso: Stream, host: &OnionV3) -> Result<Self> {
        let cfg = WebSocketConfig::default().max_message_size(Some(MASSIMO_MESSAGGIO)).max_frame_size(Some(MASSIMO_MESSAGGIO));
        let url = format!("ws://{}/relay", host.host());
        let scadenza = Instant::now() + TEMPO_MASSIMO;
        let (ws, _) = timeout_at(scadenza, tokio_tungstenite::client_async_with_config(url, flusso, Some(cfg)))
            .await
            .map_err(|_| errore("tempo massimo superato nell'apertura"))?
            .map_err(errore)?;
        Ok(Self { ws, prossimo: 0 })
    }

    /// `["REQ", id, filtro]` → eventi fino a `EOSE`, poi `["CLOSE", id]`.
    pub async fn richiedi(&mut self, filtro: Value) -> Result<Vec<Value>> {
        let id = format!("m4-{}", self.prossimo);
        self.prossimo += 1;
        let scadenza = Instant::now() + TEMPO_MASSIMO;
        self.manda(&json!(["REQ", id, filtro]), scadenza).await?;
        let mut eventi = vec![];
        loop {
            let m = timeout_at(scadenza, self.ws.next())
                .await
                .map_err(|_| errore("tempo massimo superato in attesa degli eventi"))?
                .ok_or_else(|| errore("il nodo ha chiuso la connessione"))?
                .map_err(errore)?;
            let testo = match m {
                Message::Text(t) => t,
                Message::Close(_) => return Err(errore("il nodo ha chiuso la connessione")),
                _ => continue, // ping/pong li gestisce tungstenite; binario: non previsto, ignorato
            };
            let Ok(Value::Array(a)) = serde_json::from_str::<Value>(&testo) else { continue };
            if a.get(1).and_then(Value::as_str) != Some(id.as_str()) {
                continue; // NOTICE o altre sottoscrizioni
            }
            match a.first().and_then(Value::as_str) {
                Some("EVENT") => {
                    if eventi.len() == MASSIMO_EVENTI {
                        return Err(errore(format!("più di {MASSIMO_EVENTI} eventi in una richiesta")));
                    }
                    eventi.push(a.get(2).cloned().unwrap_or(Value::Null));
                }
                Some("EOSE") => break,
                Some("CLOSED") => return Err(errore(format!("richiesta rifiutata: {}", a.get(2).and_then(Value::as_str).unwrap_or("")))),
                _ => {}
            }
        }
        self.manda(&json!(["CLOSE", id]), scadenza).await?;
        Ok(eventi)
    }

    /// Chiude la connessione (senza aspettare oltre il tempo massimo).
    pub async fn chiudi(mut self) {
        let _ = tokio::time::timeout(Duration::from_secs(5), self.ws.close(None)).await;
    }

    async fn manda(&mut self, v: &Value, scadenza: Instant) -> Result<()> {
        timeout_at(scadenza, self.ws.send(Message::text(v.to_string())))
            .await
            .map_err(|_| errore("tempo massimo superato nell'invio"))?
            .map_err(errore)
    }
}
