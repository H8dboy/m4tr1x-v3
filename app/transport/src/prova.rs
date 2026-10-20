//! SOLO PER LE PROVE (feature `prova`, mai nell'app): un relay finto che risponde su un flusso già
//! aperto, così chi usa il trasporto (es. `m4tr1x-feed`) si prova senza rete e senza Tor, e
//! `tokio-tungstenite` resta solo in questo crate (app/deny.toml).

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::tungstenite::Message;

/// Accetta il WebSocket sul flusso e a ogni `REQ` risponde con `eventi` e poi `EOSE`.
pub async fn relay_finto<S: AsyncRead + AsyncWrite + Unpin>(flusso: S, eventi: Vec<Value>) {
    let Ok(mut ws) = tokio_tungstenite::accept_async(flusso).await else { return };
    while let Some(Ok(m)) = ws.next().await {
        let Message::Text(t) = m else { continue };
        let Ok(Value::Array(a)) = serde_json::from_str::<Value>(&t) else { continue };
        if a.first().and_then(Value::as_str) == Some("REQ") {
            let id = a.get(1).cloned().unwrap_or(Value::Null);
            for e in &eventi {
                if ws.send(Message::text(json!(["EVENT", id, e]).to_string())).await.is_err() {
                    return;
                }
            }
            let _ = ws.send(Message::text(json!(["EOSE", id]).to_string())).await;
        }
    }
}
