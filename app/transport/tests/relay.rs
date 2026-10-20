//! Client del relay (src/relay.rs) contro un relay finto collegato in memoria: niente rete,
//! niente Tor. Il flusso in memoria prende il posto di quello onion di `connect_onion`.

use futures_util::{SinkExt, StreamExt};
use m4tr1x_transport::relay::{MASSIMO_EVENTI, MASSIMO_MESSAGGIO, TEMPO_MASSIMO};
use m4tr1x_transport::{OnionV3, Relay, Stream, TransportError};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::io::DuplexStream;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

type Lato = WebSocketStream<DuplexStream>;

/// Avvia un relay finto: `comportamento` riceve la connessione già aperta. Restituisce il client e
/// l'header Host visto dal relay.
async fn collega<F, Fut>(comportamento: F) -> (Result<Relay, TransportError>, Arc<Mutex<String>>)
where
    F: FnOnce(Lato) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let (client, server) = tokio::io::duplex(1 << 20);
    let host = Arc::new(Mutex::new(String::new()));
    let h = host.clone();
    tokio::spawn(async move {
        let cb = move |req: &Request, res: Response| {
            *h.lock().unwrap() = req.headers().get("host").and_then(|v| v.to_str().ok()).unwrap_or("").to_owned();
            assert_eq!(req.uri().path(), "/relay");
            Ok(res)
        };
        let ws = tokio_tungstenite::accept_hdr_async(server, cb).await.unwrap();
        comportamento(ws).await;
    });
    let flusso: Stream = Box::pin(client);
    (Relay::apri(flusso, &onion()).await, host)
}

fn onion() -> OnionV3 {
    OnionV3::from_pubkey([7; 32])
}

async fn ricevi(ws: &mut Lato) -> Value {
    loop {
        if let Message::Text(t) = ws.next().await.unwrap().unwrap() {
            return serde_json::from_str(&t).unwrap();
        }
    }
}

async fn manda(ws: &mut Lato, v: Value) {
    ws.send(Message::text(v.to_string())).await.unwrap();
}

#[tokio::test]
async fn richiesta_eventi_fino_a_eose_poi_close() {
    let chiusa = Arc::new(Mutex::new(None::<Value>));
    let c = chiusa.clone();
    let (relay, host) = collega(move |mut ws| async move {
        let req = ricevi(&mut ws).await;
        assert_eq!(req[0], "REQ");
        assert_eq!(req[2], json!({ "kinds": [1], "limit": 2 }));
        let id = req[1].clone();
        manda(&mut ws, json!(["NOTICE", "ciao"])).await;
        manda(&mut ws, json!(["EVENT", "altra", { "id": "x" }])).await; // altra sottoscrizione: ignorata
        manda(&mut ws, json!(["EVENT", id, { "id": "a" }])).await;
        manda(&mut ws, json!(["EVENT", id, { "id": "b" }])).await;
        manda(&mut ws, json!(["EOSE", id])).await;
        *c.lock().unwrap() = Some(ricevi(&mut ws).await);
    })
    .await;
    let mut relay = relay.unwrap();
    assert_eq!(*host.lock().unwrap(), onion().host(), "l'header Host deve essere l'onion");
    let eventi = relay.richiedi(json!({ "kinds": [1], "limit": 2 })).await.unwrap();
    assert_eq!(eventi, vec![json!({ "id": "a" }), json!({ "id": "b" })]);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let close = chiusa.lock().unwrap().clone().expect("il client non ha mandato CLOSE");
    assert_eq!(close[0], "CLOSE");
}

#[tokio::test]
async fn closed_dal_nodo_e_un_errore() {
    let (relay, _) = collega(|mut ws| async move {
        let id = ricevi(&mut ws).await[1].clone();
        manda(&mut ws, json!(["CLOSED", id, "blocked: troppe richieste"])).await;
    })
    .await;
    let e = relay.unwrap().richiedi(json!({})).await.unwrap_err();
    assert!(e.to_string().contains("blocked"), "{e}");
}

#[tokio::test]
async fn troppi_eventi_e_un_errore() {
    let (relay, _) = collega(|mut ws| async move {
        let id = ricevi(&mut ws).await[1].clone();
        for i in 0..=MASSIMO_EVENTI {
            manda(&mut ws, json!(["EVENT", id, { "n": i }])).await;
        }
        manda(&mut ws, json!(["EOSE", id])).await;
    })
    .await;
    let e = relay.unwrap().richiedi(json!({})).await.unwrap_err();
    assert!(e.to_string().contains(&MASSIMO_EVENTI.to_string()), "{e}");
}

#[tokio::test]
async fn messaggio_troppo_grande_e_un_errore() {
    let (relay, _) = collega(|mut ws| async move {
        let id = ricevi(&mut ws).await[1].clone();
        let _ = ws.send(Message::text(json!(["EVENT", id, { "content": "x".repeat(MASSIMO_MESSAGGIO) }]).to_string())).await;
    })
    .await;
    assert!(relay.unwrap().richiedi(json!({})).await.is_err());
}

#[tokio::test(start_paused = true)]
async fn nodo_muto_tempo_massimo() {
    let (relay, _) = collega(|mut ws| async move {
        let _ = ricevi(&mut ws).await;
        tokio::time::sleep(TEMPO_MASSIMO * 10).await; // non risponde mai
    })
    .await;
    let inizio = tokio::time::Instant::now();
    let e = relay.unwrap().richiedi(json!({})).await.unwrap_err();
    assert!(e.to_string().contains("tempo massimo"), "{e}");
    assert!(inizio.elapsed() >= TEMPO_MASSIMO && inizio.elapsed() < TEMPO_MASSIMO * 2);
}

#[tokio::test]
async fn nodo_che_chiude_e_un_errore() {
    let (relay, _) = collega(|mut ws| async move {
        let _ = ricevi(&mut ws).await;
        let _ = ws.close(None).await;
    })
    .await;
    assert!(relay.unwrap().richiedi(json!({})).await.is_err());
}

// ─── pubblica (F4, post) ─────────────────────────────────────────────────────

#[tokio::test]
async fn pubblica_aspetta_l_ok_del_proprio_evento() {
    let (relay, _) = collega(|mut ws| async move {
        let ev = ricevi(&mut ws).await;
        assert_eq!(ev[0], "EVENT");
        let id = ev[1]["id"].clone();
        manda(&mut ws, json!(["OK", "un-altro-id", false, "non è il tuo"])).await; // ignorato
        manda(&mut ws, json!(["OK", id, true, ""])).await;
    })
    .await;
    let r = relay.unwrap().pubblica(&json!({ "id": "abc", "content": "ciao" })).await.unwrap();
    assert_eq!(r, (true, String::new()));
}

#[tokio::test]
async fn il_no_del_nodo_e_una_risposta_non_un_errore() {
    let (relay, _) = collega(|mut ws| async move {
        let id = ricevi(&mut ws).await[1]["id"].clone();
        manda(&mut ws, json!(["OK", id, false, "blocked: quota di questo autore esaurita"])).await;
    })
    .await;
    let (ok, motivo) = relay.unwrap().pubblica(&json!({ "id": "abc" })).await.unwrap();
    assert!(!ok);
    assert!(motivo.starts_with("blocked"), "{motivo}");
}

#[tokio::test(start_paused = true)]
async fn pubblica_con_nodo_muto_tempo_massimo() {
    let (relay, _) = collega(|mut ws| async move {
        let _ = ricevi(&mut ws).await;
        tokio::time::sleep(TEMPO_MASSIMO * 10).await;
    })
    .await;
    let e = relay.unwrap().pubblica(&json!({ "id": "abc" })).await.unwrap_err();
    assert!(e.to_string().contains("tempo massimo"), "{e}");
}
