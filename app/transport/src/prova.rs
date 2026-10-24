//! SOLO PER LE PROVE (feature `prova`, mai nell'app): un relay finto che risponde su un flusso già
//! aperto, così chi usa il trasporto (es. `m4tr1x-feed`) si prova senza rete e senza Tor, e
//! `tokio-tungstenite` resta solo in questo crate (app/deny.toml).

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::tungstenite::Message;

/// Contenuto che il relay finto rifiuta (`OK` false), per provare il "no" del nodo.
pub const CONTENUTO_RIFIUTATO: &str = "relay finto: rifiutami";

/// Un evento passa il filtro NIP-01 (solo `kinds`, `authors`, `ids`: quelli che usa l'app).
fn passa(filtro: &Value, e: &Value) -> bool {
    let in_elenco = |chiave: &str, valore: &Value| filtro.get(chiave).and_then(Value::as_array).is_none_or(|l| l.contains(valore));
    in_elenco("kinds", &e["kind"]) && in_elenco("authors", &e["pubkey"]) && in_elenco("ids", &e["id"])
}

/// Accetta il WebSocket sul flusso e a ogni `REQ` risponde con `eventi` e poi `EOSE`.
pub async fn relay_finto<S: AsyncRead + AsyncWrite + Unpin>(flusso: S, eventi: Vec<Value>) {
    relay_finto_condiviso(flusso, Arc::new(Mutex::new(eventi))).await
}

/// Come [`relay_finto`], con gli eventi condivisi tra più connessioni: un `EVENT` accettato (`OK` true)
/// si aggiunge e le `REQ` successive lo restituiscono. `CONTENUTO_RIFIUTATO` → `OK` false.
pub async fn relay_finto_condiviso<S: AsyncRead + AsyncWrite + Unpin>(flusso: S, eventi: Arc<Mutex<Vec<Value>>>) {
    relay_finto_core(flusso, eventi, true).await
}

/// Come [`relay_finto_condiviso`] ma IGNORA i filtri della `REQ`: manda OGNI evento, anche di kind o autori
/// non chiesti, come un relay malevolo o bacato. Serve a provare le difese in profondità del client:
/// il feed deve scartare i kind != 1, i profili i kind != 0 e gli autori diversi da quello chiesto.
pub async fn relay_finto_che_ignora_il_filtro<S: AsyncRead + AsyncWrite + Unpin>(flusso: S, eventi: Arc<Mutex<Vec<Value>>>) {
    relay_finto_core(flusso, eventi, false).await
}

/// Cuore dei relay finti: `filtra` = applica il filtro NIP-01 della `REQ` (come il nodo vero) oppure manda tutto.
async fn relay_finto_core<S: AsyncRead + AsyncWrite + Unpin>(flusso: S, eventi: Arc<Mutex<Vec<Value>>>, filtra: bool) {
    let Ok(mut ws) = tokio_tungstenite::accept_async(flusso).await else { return };
    while let Some(Ok(m)) = ws.next().await {
        let Message::Text(t) = m else { continue };
        let Ok(Value::Array(a)) = serde_json::from_str::<Value>(&t) else { continue };
        match a.first().and_then(Value::as_str) {
            Some("REQ") => {
                let id = a.get(1).cloned().unwrap_or(Value::Null);
                // col nodo vero il filtro (kinds, authors, ids) vale; `filtra=false` simula un relay che manda tutto
                let filtro = a.get(2).cloned().unwrap_or(Value::Null);
                let copia: Vec<Value> = eventi.lock().unwrap().iter().filter(|e| !filtra || passa(&filtro, e)).cloned().collect();
                for e in &copia {
                    if ws.send(Message::text(json!(["EVENT", id, e]).to_string())).await.is_err() {
                        return;
                    }
                }
                let _ = ws.send(Message::text(json!(["EOSE", id]).to_string())).await;
            }
            Some("EVENT") => {
                let ev = a.get(1).cloned().unwrap_or(Value::Null);
                let id = ev.get("id").cloned().unwrap_or(Value::Null);
                let risposta = if ev.get("content").and_then(Value::as_str) == Some(CONTENUTO_RIFIUTATO) {
                    json!(["OK", id, false, "blocked: rifiutato dal relay finto"])
                } else {
                    eventi.lock().unwrap().push(ev);
                    json!(["OK", id, true, ""])
                };
                let _ = ws.send(Message::text(risposta.to_string())).await;
            }
            _ => {}
        }
    }
}
