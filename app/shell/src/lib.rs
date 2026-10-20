//! Guscio desktop di M4TR1X (F4, ADR-0002, ADR-0016).
//!
//! Divisione dei compiti (I-31): il JavaScript dell'interfaccia mostra e chiede;
//! il lato Rust tiene le chiavi, firma e parla con la rete (solo tramite
//! `m4tr1x-transport`, I-36). I comandi della finestra principale restituiscono solo
//! i tipi di `api` — mai chiavi, semi o parole. Le 24 parole esistono solo nella
//! finestra del seme (I-58), con i comandi `seme_*` e il tipo di `api_seme`.

pub mod api;
pub mod api_seme;

use api::{Identita, Stato};
use api_seme::SemeDaMostrare;
use m4tr1x_identity::creazione::{self, CreazioneInCorso};
use m4tr1x_identity::portachiavi::Sistema;
use m4tr1x_transport::{TorProcessConfig, TorProcessTransport, Transport, TransportStatus};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use zeroize::Zeroize;

const FINESTRA_SEME: &str = "seme";

/// Creazione in corso: le parole vivono solo qui (e nella finestra del seme) finché
/// l'utente non conferma o chiude la finestra; poi si cancellano (zeroize in Parole).
#[derive(Default)]
struct StatoSeme(Mutex<Option<CreazioneInCorso>>);

/// Seconda difesa oltre alla capability: i comandi del seme rispondono solo alla sua finestra.
fn solo_finestra_seme(w: &WebviewWindow) -> Result<(), String> {
    if w.label() == FINESTRA_SEME {
        Ok(())
    } else {
        Err("comando permesso solo nella finestra del seme".into())
    }
}

/// Esclude la finestra dalle catture dello schermo (screenshot, registrazioni, condivisione).
/// Su Windows 10 2004+ la finestra sparisce dalle catture; sui Windows più vecchi resta nera.
/// Se non si riesce, la finestra del seme NON si apre.
#[cfg(windows)]
fn escludi_dalle_catture(w: &WebviewWindow) -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_MONITOR};
    let hwnd = w.hwnd().map_err(|e| e.to_string())?.0;
    // SAFETY: hwnd è la finestra appena creata da Tauri in questo processo
    unsafe {
        if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) != 0 || SetWindowDisplayAffinity(hwnd, WDA_MONITOR) != 0 {
            Ok(())
        } else {
            Err("impossibile proteggere la finestra del seme dalle catture dello schermo".into())
        }
    }
}

#[cfg(not(windows))]
fn escludi_dalle_catture(_w: &WebviewWindow) -> Result<(), String> {
    // Linux/macOS: da fare nelle fasi successive (F9 per Android: FLAG_SECURE)
    Ok(())
}

/// Tor dell'app (I-37, ADR-0016). `None`: tor non indicato (in F4 la variabile `M4TR1X_TOR_BIN`)
/// oppure piattaforma che userà Arti (non ancora fatto): il trasporto resta spento e lo stato lo dice.
struct Rete(Option<Arc<TorProcessTransport>>);

/// Avvia tor in background: hash dal lock incluso alla compilazione, cartella dati dell'app.
/// Gli errori restano nello stato del trasporto (`Errore`); all'interfaccia arriva solo "errore" (api.rs).
fn avvia_rete(app: &tauri::App) -> Result<Rete, Box<dyn std::error::Error>> {
    let dati = app.path().app_data_dir()?;
    let cfg = if cfg!(windows) { TorProcessConfig::per_l_app(std::env::var_os("M4TR1X_TOR_BIN").map(PathBuf::from), dati) } else { None };
    let tor = cfg.map(|c| Arc::new(TorProcessTransport::new(c)));
    if let Some(t) = tor.clone() {
        tauri::async_runtime::spawn(async move {
            if t.avvia().await.is_ok() {
                let _ = t.bootstrap().await;
            }
        });
    }
    Ok(Rete(tor))
}

/// Stato dell'app per l'interfaccia. Nessun dato sensibile.
#[tauri::command]
fn stato(rete: tauri::State<'_, Rete>) -> Stato {
    let t = rete.0.as_ref().map(|t| t.status()).unwrap_or(TransportStatus::Spento);
    Stato::new(env!("CARGO_PKG_VERSION"), &t)
}

/// Identità salvata (solo chiavi pubbliche), se c'è.
#[tauri::command]
fn identita() -> Result<Option<Identita>, String> {
    creazione::identita_salvata(&Sistema).map(|o| o.map(Identita::from)).map_err(|e| e.to_string())
}

/// Apre la finestra del seme: invisibile, protetta dalle catture, poi mostrata.
/// `modo`: "crea" o "recupera". `async`: su Windows creare una finestra da un comando
/// sincrono blocca il thread che dovrebbe disegnarla (documentazione di Tauri 2).
#[tauri::command]
async fn apri_finestra_seme(app: tauri::AppHandle, modo: String) -> Result<(), String> {
    if modo != "crea" && modo != "recupera" {
        return Err("modo sconosciuto".into());
    }
    if let Some(w) = app.get_webview_window(FINESTRA_SEME) {
        return w.set_focus().map_err(|e| e.to_string());
    }
    let w = WebviewWindowBuilder::new(&app, FINESTRA_SEME, WebviewUrl::App(format!("seme.html?modo={modo}").into()))
        .title("M4TR1X — le tue 24 parole")
        .inner_size(720.0, 640.0)
        .resizable(false)
        .devtools(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    if let Err(e) = escludi_dalle_catture(&w) {
        let _ = w.destroy();
        return Err(e);
    }
    w.show().map_err(|e| e.to_string())
}

/// 24 parole e le 3 posizioni da verificare. Solo finestra del seme.
/// `rivedi`: rimostra le parole della creazione in corso (chi le ha già scritte non deve
/// ritrovarsi con parole nuove); altrimenti ne genera di nuove.
#[tauri::command]
fn seme_nuovo(window: WebviewWindow, stato: tauri::State<'_, StatoSeme>, rivedi: bool) -> Result<SemeDaMostrare, String> {
    solo_finestra_seme(&window)?;
    let mut g = stato.0.lock().unwrap();
    if !(rivedi && g.is_some()) {
        *g = Some(CreazioneInCorso::nuova().map_err(|e| e.to_string())?); // la precedente si cancella qui
    }
    let c = g.as_ref().expect("appena impostata");
    Ok(SemeDaMostrare {
        parole: c.parole().iter().map(|s| s.to_string()).collect(),
        posizioni: c.posizioni().iter().map(|&p| p as u8 + 1).collect(),
    })
}

/// Controlla le 3 parole; se giuste salva identity, dm, wallet e chiude la finestra.
#[tauri::command]
fn seme_conferma(window: WebviewWindow, stato: tauri::State<'_, StatoSeme>, mut risposte: Vec<String>) -> Result<Identita, String> {
    solo_finestra_seme(&window)?;
    let mut g = stato.0.lock().unwrap();
    let esito = g.as_ref().ok_or("nessuna creazione in corso")?.conferma(&risposte, &Sistema);
    risposte.iter_mut().for_each(|r| r.zeroize());
    let id = esito.map_err(|e| e.to_string())?;
    *g = None; // parole cancellate dalla memoria
    drop(g);
    let _ = window.close();
    Ok(id.into())
}

/// Recupero dalle 24 parole. Solo finestra del seme.
#[tauri::command]
fn seme_recupera(window: WebviewWindow, mut parole: String) -> Result<Identita, String> {
    solo_finestra_seme(&window)?;
    let esito = creazione::recupera(&parole, &Sistema);
    parole.zeroize();
    let id = esito.map_err(|e| e.to_string())?;
    let _ = window.close();
    Ok(id.into())
}

pub fn run() {
    tauri::Builder::default()
        .manage(StatoSeme::default())
        .setup(|app| {
            let rete = avvia_rete(app)?;
            app.manage(rete);
            Ok(())
        })
        .on_window_event(|w, ev| {
            // finestra del seme chiusa senza conferma: le parole si cancellano subito
            if w.label() == FINESTRA_SEME && matches!(ev, tauri::WindowEvent::Destroyed) {
                if let Some(s) = w.try_state::<StatoSeme>() {
                    *s.0.lock().unwrap() = None;
                }
            }
        })
        .invoke_handler(tauri::generate_handler![stato, identita, apri_finestra_seme, seme_nuovo, seme_conferma, seme_recupera])
        .build(tauri::generate_context!())
        .expect("avvio dell'app fallito")
        .run(|app, ev| {
            // l'app si chiude: tor si chiude con lei (oltre a TAKEOWNERSHIP, che lo chiude anche se l'app muore)
            if let RunEvent::Exit = ev {
                if let Some(t) = app.try_state::<Rete>().and_then(|r| r.0.clone()) {
                    tauri::async_runtime::block_on(t.shutdown());
                }
            }
        });
}
