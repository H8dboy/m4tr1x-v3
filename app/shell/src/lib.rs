//! Guscio desktop di M4TR1X (F4, ADR-0002, ADR-0016).
//!
//! Divisione dei compiti (I-31): il JavaScript dell'interfaccia mostra e chiede;
//! il lato Rust tiene le chiavi, firma e parla con la rete (solo tramite
//! `m4tr1x-transport`, I-36). I comandi della finestra principale restituiscono solo
//! i tipi di `api` — mai chiavi, semi o parole. Le 24 parole esistono solo nella
//! finestra del seme (I-58), con i comandi `seme_*` e il tipo di `api_seme`.

pub mod api;
pub mod api_seme;

use api::{Avanzamento, Feed, Identita, Post, Stato};
use api_seme::SemeDaMostrare;
use m4tr1x_identity::creazione::{self, CreazioneInCorso};
use m4tr1x_identity::portachiavi::{firma_come_identita, Sistema};
use m4tr1x_transport::{OnionV3, TorProcessConfig, TorProcessTransport, Transport, TransportStatus};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// Tor dell'app (I-37, ADR-0016) e nodo di prova. `tor` None: tor non indicato (in F4 la variabile
/// `M4TR1X_TOR_BIN`) oppure piattaforma che userà Arti (non ancora fatto). `nodo` None: variabile
/// `M4TR1X_NODO_PROVA` assente o non un onion v3 valido. L'onion NON esce mai verso l'interfaccia.
struct Rete {
    tor: Option<Arc<TorProcessTransport>>,
    nodo: Option<OnionV3>,
}

/// Un'operazione verso il nodo (leggere il feed, pubblicare un post): una alla volta, con la fase letta
/// dall'interfaccia ogni secondo (`*_avanzamento`): niente eventi di Tauri, servirebbe un permesso core (I-31).
#[derive(Default)]
struct Operazione {
    in_corso: AtomicBool,
    fase: Mutex<Option<Avanzamento>>,
}

impl Operazione {
    fn imposta(&self, a: Avanzamento) {
        *self.fase.lock().unwrap() = Some(a);
    }
    fn fase(&self) -> Avanzamento {
        self.fase.lock().unwrap().clone().unwrap_or_else(Avanzamento::fermo)
    }
}

#[derive(Default)]
struct Operazioni {
    feed: Operazione,
    post: Operazione,
}

/// Aspetta che tor sia pronto (al massimo il tempo di bootstrap del trasporto), segnalando la fase.
/// "Spento" con tor configurato = sta per partire (app appena aperta): si aspetta; si smette solo su errore.
async fn attendi_tor(tor: &TorProcessTransport, op: &Operazione) -> Result<(), String> {
    let t0 = std::time::Instant::now();
    while !matches!(tor.status(), TransportStatus::Pronto) {
        if matches!(tor.status(), TransportStatus::Errore(_)) || t0.elapsed() > std::time::Duration::from_secs(180) {
            return Err("tor non è pronto".to_string());
        }
        op.imposta(Avanzamento::tor(t0.elapsed().as_secs()));
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Ok(())
}

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
    let nodo = std::env::var("M4TR1X_NODO_PROVA").ok().and_then(|s| OnionV3::parse(s.trim().trim_start_matches("http://")).ok());
    Ok(Rete { tor, nodo })
}

/// Stato dell'app per l'interfaccia. Nessun dato sensibile.
#[tauri::command]
fn stato(rete: tauri::State<'_, Rete>) -> Stato {
    let t = rete.tor.as_ref().map(|t| t.status()).unwrap_or(TransportStatus::Spento);
    Stato::new(env!("CARGO_PKG_VERSION"), &t)
}

/// Ultimi post del nodo di prova, verificati nel lato Rust (m4tr1x-feed). Aspetta tor pronto, poi
/// tentativi da 30 s fino a 4 minuti; la fase si legge con `feed_avanzamento`.
/// Gli errori dicono cosa manca o quanto si è aspettato, mai l'indirizzo del nodo.
#[tauri::command]
async fn feed(rete: tauri::State<'_, Rete>, op: tauri::State<'_, Operazioni>) -> Result<Feed, String> {
    let tor = rete.tor.clone().ok_or("tor non configurato (M4TR1X_TOR_BIN)")?;
    let nodo = rete.nodo.clone().ok_or("nodo di prova non configurato (M4TR1X_NODO_PROVA)")?;
    let op = &op.feed;
    if op.in_corso.swap(true, Ordering::SeqCst) {
        return Err("feed già in aggiornamento".into());
    }
    let esito = async {
        attendi_tor(&tor, op).await?;
        // budget scaduto → attesa di un minuto con conto alla rovescia → una ripresa (fondatore, 20/10/2026)
        m4tr1x_feed::leggi_con_ripresa(&*tor, &nodo, m4tr1x_protocol::Rete::Prova, m4tr1x_feed::BUDGET, |f| op.imposta(Avanzamento::from(&f)))
            .await
            .map(Feed::from)
            .map_err(|e| format!("il nodo non risponde ancora: Tor continua a cercarlo. Riprova tra un minuto ({})", e.ultimo))
    }
    .await;
    op.in_corso.store(false, Ordering::SeqCst);
    esito
}

/// Fase del feed in corso (o dell'ultimo), per l'interfaccia.
#[tauri::command]
fn feed_avanzamento(op: tauri::State<'_, Operazioni>) -> Avanzamento {
    op.feed.fase()
}

/// Pubblica un post firmato con la TUA identità.
/// La chiave d'identità si legge dal portachiavi SOLO dentro la firma (`firma_come_identita`) e si
/// cancella appena firmato: qui non esiste mai una `ChiaveRuolo` (fondatore, 20/10/2026). L'evento
/// firmato si riverifica (`firma_evento`) prima di partire. All'interfaccia torna solo il post pubblico.
#[tauri::command]
async fn pubblica(testo: String, rete: tauri::State<'_, Rete>, op: tauri::State<'_, Operazioni>) -> Result<Post, String> {
    let testo = testo.trim().to_string();
    if testo.is_empty() {
        return Err("il post è vuoto".into());
    }
    if testo.chars().count() > m4tr1x_protocol::MASSIMO_TESTO_POST {
        return Err(format!("il post supera i {} caratteri", m4tr1x_protocol::MASSIMO_TESTO_POST));
    }
    let tor = rete.tor.clone().ok_or("tor non configurato (M4TR1X_TOR_BIN)")?;
    let nodo = rete.nodo.clone().ok_or("nodo di prova non configurato (M4TR1X_NODO_PROVA)")?;
    let identita = creazione::identita_salvata(&Sistema).map_err(|e| e.to_string())?.ok_or("crea prima la tua identità")?;
    let ora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
    let evento = m4tr1x_protocol::firma_evento(1, &testo, &[], ora, m4tr1x_protocol::Rete::Prova, &identita.identity, |d| {
        firma_come_identita(&Sistema, d).ok()
    })
    .map_err(|e| format!("post non firmato: {e}"))?;
    let op = &op.post;
    if op.in_corso.swap(true, Ordering::SeqCst) {
        return Err("un post è già in pubblicazione".into());
    }
    let esito = async {
        attendi_tor(&tor, op).await?;
        m4tr1x_feed::pubblica(&*tor, &nodo, &evento, m4tr1x_feed::BUDGET, |f| op.imposta(Avanzamento::from(&f))).await.map_err(|e| match e {
            m4tr1x_feed::ErrorePubblica::Rifiutato(m) => format!("il nodo ha rifiutato il post: {m}"),
            m4tr1x_feed::ErrorePubblica::NonRaggiungibile(n) => {
                format!("il post non è partito: il nodo non risponde ancora. Il testo resta qui, riprova tra un minuto ({})", n.ultimo)
            }
        })
    }
    .await;
    op.in_corso.store(false, Ordering::SeqCst);
    esito.map(|_| Post { id: evento.id, autore: evento.pubkey, creato: evento.created_at, testo: evento.content })
}

/// Fase della pubblicazione in corso (o dell'ultima), per l'interfaccia.
#[tauri::command]
fn pubblica_avanzamento(op: tauri::State<'_, Operazioni>) -> Avanzamento {
    op.post.fase()
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

/// Identificatore dell'app (uguale a `identifier` in tauri.conf.json: lo controlla tests/unit/crash-log.test.js).
const IDENTIFICATORE: &str = "org.m4tr1x.app";

pub fn run() {
    // PRIMA di tutto: un crash dell'avvio (anche dentro Tauri) deve finire nel registro (m4tr1x-crash)
    let cartella_crash = m4tr1x_crash::cartella_predefinita(IDENTIFICATORE);
    if let Some(c) = cartella_crash.clone() {
        m4tr1x_crash::installa(c, env!("CARGO_PKG_VERSION"));
    }
    tauri::Builder::default()
        .manage(StatoSeme::default())
        .manage(Operazioni::default())
        .setup(move |app| {
            // se Tauri usa un'altra cartella dati, il registro la segue
            let dati = app.path().app_data_dir()?;
            if cartella_crash.as_deref() != Some(dati.as_path()) {
                m4tr1x_crash::installa(dati, env!("CARGO_PKG_VERSION"));
            }
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
        .invoke_handler(tauri::generate_handler![stato, identita, apri_finestra_seme, seme_nuovo, seme_conferma, seme_recupera, feed, feed_avanzamento, pubblica, pubblica_avanzamento])
        .build(tauri::generate_context!())
        .expect("avvio dell'app fallito")
        .run(|app, ev| {
            // l'app si chiude: tor si chiude con lei (oltre a TAKEOWNERSHIP, che lo chiude anche se l'app muore)
            if let RunEvent::Exit = ev {
                if let Some(t) = app.try_state::<Rete>().and_then(|r| r.tor.clone()) {
                    tauri::async_runtime::block_on(t.shutdown());
                }
            }
        });
}
