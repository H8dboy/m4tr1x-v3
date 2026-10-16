//! Guscio desktop di M4TR1X (F4, ADR-0002, ADR-0016).
//!
//! Divisione dei compiti (I-31): il JavaScript dell'interfaccia mostra e chiede;
//! il lato Rust tiene le chiavi, firma e parla con la rete (solo tramite
//! `m4tr1x-transport`, I-36). I comandi esposti al JavaScript restituiscono solo
//! i tipi di `api` — mai chiavi, semi o segreti.

pub mod api;

use api::Stato;
use m4tr1x_transport::TransportStatus;

/// Stato dell'app per l'interfaccia. Nessun dato sensibile.
#[tauri::command]
fn stato() -> Stato {
    Stato::new(env!("CARGO_PKG_VERSION"), &TransportStatus::Spento)
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![stato])
        .run(tauri::generate_context!())
        .expect("avvio dell'app fallito");
}
