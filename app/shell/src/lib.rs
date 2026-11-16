//! Guscio desktop di M4TR1X (F4, ADR-0002, ADR-0016).
//!
//! Divisione dei compiti (I-31): il JavaScript dell'interfaccia mostra e chiede;
//! il lato Rust tiene le chiavi, firma e parla con la rete (solo tramite
//! `m4tr1x-transport`, I-36). I comandi della finestra principale restituiscono solo
//! i tipi di `api` — mai chiavi, semi o parole. Le 24 parole esistono solo nella
//! finestra del seme (I-58), con i comandi `seme_*` e il tipo di `api_seme`.

pub mod api;
pub mod api_seme;

use api::{Avanzamento, CellePronte, CelleStato, Discussione, DiscussioneAperta, Feed, FilePronto, Forum, Galleria, Identita, Media, Messaggio, Post, Posta, Profilo, Stato};
use api_seme::SemeDaMostrare;
use m4tr1x_identity::creazione::{self, CreazioneInCorso};
use m4tr1x_identity::portachiavi::{firma_come_identita, Portachiavi, Sistema};
use m4tr1x_transport::{OnionV3, TorProcessConfig, TorProcessTransport, Transport, TransportStatus};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Builder, Manager, RunEvent, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use zeroize::Zeroize;

const FINESTRA_SEME: &str = "seme";

/// Il portachiavi dell'app: quello del sistema in produzione (`run`), uno in memoria nelle prove
/// dinamiche di I-31 (`costruisci`), così le prove non toccano mai il portachiavi vero di Windows.
pub struct Chiavi(pub Arc<dyn Portachiavi>);

/// Creazione in corso: le parole vivono solo qui (e nella finestra del seme) finché
/// l'utente non conferma o chiude la finestra; poi si cancellano (zeroize in Parole).
#[derive(Default)]
struct StatoSeme(Mutex<Option<CreazioneInCorso>>);

/// Seconda difesa oltre alla capability: i comandi del seme rispondono solo alla sua finestra.
fn solo_finestra_seme<R: Runtime>(w: &WebviewWindow<R>) -> Result<(), String> {
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
fn escludi_dalle_catture<R: Runtime>(w: &WebviewWindow<R>) -> Result<(), String> {
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
fn escludi_dalle_catture<R: Runtime>(_w: &WebviewWindow<R>) -> Result<(), String> {
    // Linux/macOS: da fare nelle fasi successive (F9 per Android: FLAG_SECURE)
    Ok(())
}

/// Tor dell'app (I-37, ADR-0016) e nodo a cui collegarsi. `tor` None: tor non indicato (in F4 la variabile
/// `M4TR1X_TOR_BIN`) oppure piattaforma che userà Arti (non ancora fatto). `nodo`: quello scelto da chi usa
/// l'app (file `nodo` nella cartella dati), oppure `M4TR1X_NODO_PROVA` per le prove, che ha la precedenza.
/// Si accetta SOLO un onion v3 valido (I-36). L'onion NON esce mai verso l'interfaccia: lei sa solo se c'è.
struct Rete {
    tor: Option<Arc<TorProcessTransport>>,
    nodo: Mutex<Option<OnionV3>>,
    /// file dove si salva il nodo scelto (None nelle prove: niente da scrivere su disco)
    file_nodo: Option<PathBuf>,
    /// nodo dei file (hub, ADR-0006): dove si caricano i pezzi e dove si cercano per primi. None = lo stesso nodo.
    /// Così le macchine piccole (un nodo su una SIM) tengono solo testi, e i file stanno dove c'è banda.
    nodo_file: Mutex<Option<OnionV3>>,
    file_nodo_file: Option<PathBuf>,
}

impl Rete {
    fn nodo(&self) -> Result<OnionV3, String> {
        self.nodo.lock().unwrap().clone().ok_or_else(|| "nessun nodo impostato: aggiungine uno nella sezione Rete".to_string())
    }
    /// Dove si caricano i file: il nodo dei file se impostato, altrimenti il nodo.
    fn nodo_dei_file(&self) -> Result<OnionV3, String> {
        match self.nodo_file.lock().unwrap().clone() {
            Some(h) => Ok(h),
            None => self.nodo(),
        }
    }
    /// Da dove si scaricano i pezzi, in ordine: prima il nodo dei file, poi il nodo (una volta sola se coincidono).
    fn fonti(&self) -> Result<Vec<OnionV3>, String> {
        let nodo = self.nodo()?;
        let mut fonti = vec![];
        if let Some(h) = self.nodo_file.lock().unwrap().clone() {
            if h.host() != nodo.host() {
                fonti.push(h);
            }
        }
        fonti.push(nodo);
        Ok(fonti)
    }
}

/// Legge un indirizzo di nodo scritto da una persona: con o senza `http://` e `/` finale, maiuscole comprese.
/// Tutto il resto (host in chiaro, onion v2, caratteri estranei, porte, percorsi) è rifiutato da `OnionV3::parse`.
fn indirizzo_nodo(s: &str) -> Option<OnionV3> {
    let t = s.trim();
    let t = t.strip_prefix("http://").unwrap_or(t);
    let t = t.strip_suffix('/').unwrap_or(t);
    OnionV3::parse(&t.to_ascii_lowercase()).ok()
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
    profilo: Operazione,
    posta: Operazione,
    forum: Operazione,
    media: Operazione,
}

/// Il file scelto per la pubblicazione, GIÀ pulito (I-16): solo il contenuto, mai il nome né il percorso.
#[derive(Default)]
struct FileScelto(Mutex<Option<(Vec<u8>, &'static str)>>);

/// Una foto o un reel a celle che si sta guardando: i pezzi arrivano via Tor uno alla volta e un filo a parte li
/// decodifica man mano; l'interfaccia legge stato e pixel ogni mezzo secondo.
#[derive(Default)]
struct Visto {
    /// cambia a ogni `celle_apri`: i fili di un'apertura vecchia si fermano
    generazione: u32,
    mime: String,
    larghezza: u32,
    altezza: u32,
    pezzi_totali: u32,
    /// i pezzi arrivati e verificati, in ordine
    pezzi: Vec<Vec<u8>>,
    /// quanti pezzi il filo che decodifica ha già usato
    decodificati: usize,
    byte_arrivati: u64,
    /// i fotogrammi decodificati (uno per una foto, rifatto a ogni pezzo), RGBA
    fotogrammi: Vec<Vec<u8>>,
    fps: u8,
    /// tutti i pezzi sono scesi e decodificati (o si è fermato per un errore)
    finito: bool,
    errore: Option<String>,
}

/// Lo stato dell'alveare: il pacchetto codificato pronto da pubblicare (i pezzi, mai il file né il nome) e quello
/// che si sta guardando. `op`: una codifica o un'apertura alla volta, con la fase per l'interfaccia.
#[derive(Default)]
struct Alveare {
    pronto: Mutex<Option<(Vec<Vec<u8>>, &'static str)>>,
    visto: Arc<Mutex<Visto>>,
    in_corso: Arc<AtomicBool>,
    fase: Arc<Mutex<Option<Avanzamento>>>,
}

impl Alveare {
    fn imposta(&self, a: Avanzamento) {
        *self.fase.lock().unwrap() = Some(a);
    }
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

/// Il tor incluso nell'installer (`tools/tor/prepara-installer.js` lo mette nelle risorse `tor/`), se c'è.
fn tor_incluso<R: Runtime>(app: &tauri::App<R>) -> Option<PathBuf> {
    let nome = if cfg!(windows) { "tor.exe" } else { "tor" };
    let p = app.path().resource_dir().ok()?.join("tor").join(nome);
    p.is_file().then_some(p)
}

/// Avvia tor in background: hash dal lock incluso alla compilazione, cartella dati dell'app.
/// Gli errori restano nello stato del trasporto (`Errore`); all'interfaccia arriva solo "errore" (api.rs).
fn avvia_rete<R: Runtime>(app: &tauri::App<R>) -> Result<Rete, Box<dyn std::error::Error>> {
    let dati = app.path().app_data_dir()?;
    let dati_app = dati.clone();
    // desktop = tor come processo (ADR-0016 aggiornata il 21/10/2026); mobile = Arti, non ancora fatto
    let desktop = cfg!(any(windows, target_os = "linux", target_os = "macos"));
    // tor: quello indicato da M4TR1X_TOR_BIN (sviluppo e prove), altrimenti quello incluso nell'installer
    // (risorsa `tor/`). In entrambi i casi l'hash si controlla contro il lock PRIMA di usarlo (I-37):
    // un tor cambiato nella cartella d'installazione non parte.
    let tor_bin = std::env::var_os("M4TR1X_TOR_BIN").map(PathBuf::from).or_else(|| tor_incluso(app));
    let cfg = if desktop { TorProcessConfig::per_l_app(tor_bin, dati) } else { None };
    let tor = cfg.map(|c| Arc::new(TorProcessTransport::new(c)));
    if let Some(t) = tor.clone() {
        tauri::async_runtime::spawn(async move {
            if t.avvia().await.is_ok() {
                let _ = t.bootstrap().await;
            }
        });
    }
    let file_nodo = dati_app.join("nodo");
    let nodo = match std::env::var("M4TR1X_NODO_PROVA") {
        Ok(s) => indirizzo_nodo(&s),
        Err(_) => std::fs::read_to_string(&file_nodo).ok().and_then(|s| indirizzo_nodo(&s)),
    };
    let file_nodo_file = dati_app.join("nodo-file");
    let nodo_file = std::fs::read_to_string(&file_nodo_file).ok().and_then(|s| indirizzo_nodo(&s));
    Ok(Rete { tor, nodo: Mutex::new(nodo), file_nodo: Some(file_nodo), nodo_file: Mutex::new(nodo_file), file_nodo_file: Some(file_nodo_file) })
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
    let nodo = rete.nodo()?;
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

/// C'è un nodo impostato? All'interfaccia arriva solo sì/no, mai l'indirizzo.
#[tauri::command]
fn nodo_impostato(rete: tauri::State<'_, Rete>) -> bool {
    rete.nodo.lock().unwrap().is_some()
}

/// Imposta il nodo a cui collegarsi: solo un indirizzo onion v3 valido (I-36). Si salva nella cartella dati
/// dell'app, leggibile solo dall'utente; vale da subito e ai prossimi avvii.
#[tauri::command]
fn nodo_imposta(indirizzo: String, rete: tauri::State<'_, Rete>) -> Result<(), String> {
    let nodo = indirizzo_nodo(&indirizzo).ok_or("indirizzo non valido: serve l'indirizzo .onion (v3) del nodo")?;
    if let Some(f) = &rete.file_nodo {
        salva_privato(f, &format!("{}\n", nodo.host())).map_err(|e| format!("nodo non salvato: {e}"))?;
    }
    *rete.nodo.lock().unwrap() = Some(nodo);
    Ok(())
}

/// C'è un nodo dei file (hub) diverso dal nodo? L'interfaccia sa solo questo, mai l'indirizzo.
#[tauri::command]
fn nodo_file_impostato(rete: tauri::State<'_, Rete>) -> bool {
    rete.nodo_file.lock().unwrap().is_some()
}

/// Imposta il nodo dei file (hub): dove si caricano i file e dove si cercano per primi. Solo un onion v3 valido
/// (I-36); vuoto = torna a usare il nodo anche per i file. Si salva come il nodo (`nodo-file`, solo proprietario).
#[tauri::command]
fn nodo_file_imposta(indirizzo: String, rete: tauri::State<'_, Rete>) -> Result<(), String> {
    if indirizzo.trim().is_empty() {
        if let Some(f) = &rete.file_nodo_file {
            let _ = std::fs::remove_file(f);
        }
        *rete.nodo_file.lock().unwrap() = None;
        return Ok(());
    }
    let hub = indirizzo_nodo(&indirizzo).ok_or("indirizzo non valido: serve l'indirizzo .onion (v3) del nodo dei file")?;
    if let Some(f) = &rete.file_nodo_file {
        salva_privato(f, &format!("{}\n", hub.host())).map_err(|e| format!("nodo dei file non salvato: {e}"))?;
    }
    *rete.nodo_file.lock().unwrap() = Some(hub);
    Ok(())
}

/// Scrive `testo` in `f` in modo atomico (file temporaneo + rinomina), leggibile solo dal proprietario.
fn salva_privato(f: &std::path::Path, testo: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(c) = f.parent() {
        std::fs::create_dir_all(c)?;
    }
    let tmp = f.with_extension("tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let mut w = o.open(&tmp)?;
    w.write_all(testo.as_bytes())?;
    w.sync_all()?;
    drop(w);
    std::fs::rename(&tmp, f)
}

/// Fase del feed in corso (o dell'ultimo), per l'interfaccia.
#[tauri::command]
fn feed_avanzamento(op: tauri::State<'_, Operazioni>) -> Avanzamento {
    op.feed.fase()
}

/// L'UNICO punto del guscio che fa firmare la tua identità (post e profilo): la chiave si legge dal
/// portachiavi SOLO dentro `firma_come_identita` e si cancella appena firmato (fondatore, 20/10/2026).
/// L'evento firmato si riverifica (`firma_evento`) prima di uscire.
fn firma_con_la_mia_identita(chiavi: &Chiavi, kind: u16, contenuto: &str, tags: &[Vec<String>]) -> Result<m4tr1x_protocol::Evento, String> {
    let identita = creazione::identita_salvata(&*chiavi.0).map_err(|e| e.to_string())?.ok_or("crea prima la tua identità")?;
    let ora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
    m4tr1x_protocol::firma_evento(kind, contenuto, tags, ora, m4tr1x_protocol::Rete::Prova, &identita.identity, |d| {
        firma_come_identita(&*chiavi.0, d).ok()
    })
    .map_err(|e| e.to_string())
}

/// Pubblica un post firmato con la TUA identità.
/// La chiave d'identità si legge dal portachiavi SOLO dentro la firma (`firma_come_identita`) e si
/// cancella appena firmato: qui non esiste mai una `ChiaveRuolo` (fondatore, 20/10/2026). L'evento
/// firmato si riverifica (`firma_evento`) prima di partire. All'interfaccia torna solo il post pubblico.
#[tauri::command]
async fn pubblica(testo: String, rete: tauri::State<'_, Rete>, op: tauri::State<'_, Operazioni>, chiavi: tauri::State<'_, Chiavi>) -> Result<Post, String> {
    let testo = testo.trim().to_string();
    if testo.is_empty() {
        return Err("il post è vuoto".into());
    }
    if testo.chars().count() > m4tr1x_protocol::MASSIMO_TESTO_POST {
        return Err(format!("il post supera i {} caratteri", m4tr1x_protocol::MASSIMO_TESTO_POST));
    }
    let tor = rete.tor.clone().ok_or("tor non configurato (M4TR1X_TOR_BIN)")?;
    let nodo = rete.nodo()?;
    let evento = firma_con_la_mia_identita(&chiavi, 1, &testo, &[]).map_err(|e| format!("post non firmato: {e}"))?;
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
    esito.map(|_| Post { id: evento.id, autore: evento.pubkey, creato: evento.created_at, testo: evento.content, nome: None })
}

/// Il TUO profilo com'è sul nodo (per precompilare il modulo). Ok(None): non ne hai ancora uno.
#[tauri::command]
async fn profilo_mio(rete: tauri::State<'_, Rete>, op: tauri::State<'_, Operazioni>, chiavi: tauri::State<'_, Chiavi>) -> Result<Option<Profilo>, String> {
    let tor = rete.tor.clone().ok_or("tor non configurato (M4TR1X_TOR_BIN)")?;
    let nodo = rete.nodo()?;
    let identita = creazione::identita_salvata(&*chiavi.0).map_err(|e| e.to_string())?.ok_or("crea prima la tua identità")?;
    let op = &op.profilo;
    if op.in_corso.swap(true, Ordering::SeqCst) {
        return Err("il profilo è già in aggiornamento".into());
    }
    let esito = async {
        attendi_tor(&tor, op).await?;
        m4tr1x_feed::leggi_profilo(&*tor, &nodo, m4tr1x_protocol::Rete::Prova, &identita.identity, m4tr1x_feed::BUDGET, |f| op.imposta(Avanzamento::from(&f)))
            .await
            .map(|p| p.map(Profilo::from))
            .map_err(|e| format!("il nodo non risponde ancora: Tor continua a cercarlo. Riprova tra un minuto ({})", e.ultimo))
    }
    .await;
    op.in_corso.store(false, Ordering::SeqCst);
    esito
}

/// Salva il TUO profilo (solo nome e descrizione), firmato con la tua identità e pubblicato sul nodo.
#[tauri::command]
async fn profilo_salva(nome: String, descrizione: String, rete: tauri::State<'_, Rete>, op: tauri::State<'_, Operazioni>, chiavi: tauri::State<'_, Chiavi>) -> Result<Profilo, String> {
    let contenuto = m4tr1x_protocol::contenuto_profilo(&nome, &descrizione).map_err(|e| e.to_string())?;
    let tor = rete.tor.clone().ok_or("tor non configurato (M4TR1X_TOR_BIN)")?;
    let nodo = rete.nodo()?;
    let evento = firma_con_la_mia_identita(&chiavi, 0, &contenuto, &[]).map_err(|e| format!("profilo non firmato: {e}"))?;
    let op = &op.profilo;
    if op.in_corso.swap(true, Ordering::SeqCst) {
        return Err("il profilo è già in aggiornamento".into());
    }
    let esito = async {
        attendi_tor(&tor, op).await?;
        m4tr1x_feed::pubblica(&*tor, &nodo, &evento, m4tr1x_feed::BUDGET, |f| op.imposta(Avanzamento::from(&f))).await.map_err(|e| match e {
            m4tr1x_feed::ErrorePubblica::Rifiutato(m) => format!("il nodo ha rifiutato il profilo: {m}"),
            m4tr1x_feed::ErrorePubblica::NonRaggiungibile(n) => format!("profilo non salvato: il nodo non risponde ancora. Riprova tra un minuto ({})", n.ultimo),
        })
    }
    .await;
    op.in_corso.store(false, Ordering::SeqCst);
    esito?;
    // quello che il nodo mostrerà: riletto dall'evento firmato con la stessa pulizia
    Ok(m4tr1x_protocol::profilo_da_evento(&evento).map(Profilo::from).ok_or("profilo non leggibile")?)
}

/// Fase del profilo in corso (lettura o salvataggio), per l'interfaccia.
#[tauri::command]
fn profilo_avanzamento(op: tauri::State<'_, Operazioni>) -> Avanzamento {
    op.profilo.fase()
}

// ─── Reels, film, musica ─────────────────────────────────────────────────────
// Si pubblica in due passi: `media_prepara` riceve i byte del file scelto (corpo grezzo dell'IPC, niente nome né
// percorso) e lo PULISCE qui (I-16); `media_pubblica` firma l'autorizzazione, carica il file pulito, firma e
// pubblica la scheda. Si guarda con `media` (schede verificate) e `media_file` (byte verificati per hash e tipo).

fn sezione_da(s: &str) -> Result<m4tr1x_protocol::Sezione, String> {
    m4tr1x_protocol::Sezione::da_nome(s).ok_or_else(|| "sezione sconosciuta".to_string())
}

/// Le schede di una sezione ("signal", "film", "musica").
#[tauri::command]
async fn media(sezione: String, rete: tauri::State<'_, Rete>, op: tauri::State<'_, Operazioni>) -> Result<Galleria, String> {
    let sez = sezione_da(&sezione)?;
    let tor = rete.tor.clone().ok_or("tor non configurato (M4TR1X_TOR_BIN)")?;
    let nodo = rete.nodo()?;
    let op = &op.media;
    if op.in_corso.swap(true, Ordering::SeqCst) {
        return Err("un'altra operazione sui file è in corso".into());
    }
    let esito = async {
        attendi_tor(&tor, op).await?;
        m4tr1x_feed::leggi_media(&*tor, &nodo, m4tr1x_protocol::Rete::Prova, sez, m4tr1x_feed::BUDGET, |f| op.imposta(Avanzamento::from(&f)))
            .await
            .map(Galleria::from)
            .map_err(|e| format!("il nodo non risponde ancora: Tor continua a cercarlo. Riprova tra un minuto ({})", e.ultimo))
    }
    .await;
    op.in_corso.store(false, Ordering::SeqCst);
    esito
}

/// Il tipo trovato nei primi byte corrisponde a quello dichiarato dalla scheda?
fn tipo_coerente(b: &[u8], mime: &str) -> bool {
    use m4tr1x_media::Tipo;
    matches!((m4tr1x_media::tipo(b), mime), (Some(Tipo::Jpeg), "image/jpeg") | (Some(Tipo::Png), "image/png") | (Some(Tipo::Mp4), "video/mp4" | "audio/mp4") | (Some(Tipo::Mp3), "audio/mpeg"))
}

/// I byte di un file, scaricati via Tor pezzo per pezzo (prima dal nodo dei file, poi dal nodo) e accettati SOLO se
/// ogni pezzo e il file ricomposto hanno l'hash della scheda (I-17) e i primi byte sono del tipo dichiarato.
/// Tornano grezzi all'interfaccia, che li mostra con un indirizzo `blob:` del tipo della scheda.
#[tauri::command]
async fn media_file(sha256: String, mime: String, dimensione: u64, pezzi: Vec<String>, rete: tauri::State<'_, Rete>, op: tauri::State<'_, Operazioni>) -> Result<tauri::ipc::Response, String> {
    if sha256.len() != 64 || !sha256.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) {
        return Err("hash del file non valido".into());
    }
    if !m4tr1x_protocol::MIME_AMMESSI.contains(&mime.as_str()) || dimensione == 0 || dimensione > m4tr1x_protocol::MASSIMO_FILE || !m4tr1x_protocol::pezzi_validi(dimensione, &pezzi) {
        return Err("scheda del file non valida".into());
    }
    let tor = rete.tor.clone().ok_or("tor non configurato (M4TR1X_TOR_BIN)")?;
    let fonti = rete.fonti()?;
    let op = &op.media;
    if op.in_corso.swap(true, Ordering::SeqCst) {
        return Err("un'altra operazione sui file è in corso".into());
    }
    let esito = async {
        attendi_tor(&tor, op).await?;
        m4tr1x_feed::scarica_file_a_pezzi(&*tor, &fonti, &sha256, dimensione, &pezzi, |f| op.imposta(Avanzamento::from(&f))).await.map_err(|e| match e {
            m4tr1x_feed::ErroreScarico::Scheda => "scheda del file non valida".to_string(),
            m4tr1x_feed::ErroreScarico::NessunaFonte { pezzo, ultimo } => format!("file non scaricato: nessun nodo ha dato il pezzo {pezzo} ({ultimo})"),
            m4tr1x_feed::ErroreScarico::NonCorrisponde => "il file ricevuto non è quello della scheda (hash diverso): scartato".to_string(),
        })
    }
    .await;
    op.in_corso.store(false, Ordering::SeqCst);
    let b = esito?;
    if !tipo_coerente(&b, &mime) {
        return Err("il file non è del tipo che dichiara la scheda: scartato".into());
    }
    Ok(tauri::ipc::Response::new(b))
}

/// Riceve il file scelto (corpo grezzo dell'IPC) e lo pulisce SUL computer (I-16). Il file pulito resta qui
/// in attesa di `media_pubblica`; all'interfaccia tornano solo tipo e dimensioni.
#[tauri::command]
fn media_prepara(request: tauri::ipc::Request<'_>, scelto: tauri::State<'_, FileScelto>) -> Result<FilePronto, String> {
    let tauri::ipc::InvokeBody::Raw(b) = request.body() else { return Err("serve il contenuto del file".into()) };
    let prima = b.len() as u64;
    let (tipo, pulito) = m4tr1x_media::ripulisci(b).map_err(|e| e.to_string())?;
    let mime = tipo.mime(tipo == m4tr1x_media::Tipo::Mp4 && m4tr1x_media::mp4_solo_audio(&pulito));
    let pronto = FilePronto { mime: mime.into(), dimensione: pulito.len() as u64, prima };
    *scelto.0.lock().unwrap() = Some((pulito, mime));
    Ok(pronto)
}

/// Pubblica il file preparato nella sezione, con titolo e didascalia: il file PULITO parte a pezzi verso il nodo
/// dei file (ADR-0005: ogni pezzo con la sua autorizzazione firmata, valida 10 minuti, firmata solo quando quel
/// pezzo serve davvero), poi la scheda firmata va al nodo. La firma passa da `firma_con_la_mia_identita`.
#[tauri::command]
async fn media_pubblica(sezione: String, titolo: String, testo: String, rete: tauri::State<'_, Rete>, op: tauri::State<'_, Operazioni>, chiavi: tauri::State<'_, Chiavi>, scelto: tauri::State<'_, FileScelto>) -> Result<Media, String> {
    let sez = sezione_da(&sezione)?;
    if testo.chars().count() > m4tr1x_protocol::MASSIMO_TESTO_POST {
        return Err(format!("la didascalia supera i {} caratteri", m4tr1x_protocol::MASSIMO_TESTO_POST));
    }
    let (file, mime) = scelto.0.lock().unwrap().clone().ok_or("scegli prima un file")?;
    let sha = m4tr1x_transport::http::sha256_hex(&file);
    let pezzi = m4tr1x_feed::hash_dei_pezzi(&file);
    let tags = m4tr1x_protocol::tag_file(sez, &sha, mime, file.len() as u64, &titolo, &pezzi).map_err(|e| match e {
        m4tr1x_protocol::Rifiuto::Forma(m) => m.to_string(),
        altro => altro.to_string(),
    })?;
    let tor = rete.tor.clone().ok_or("tor non configurato (M4TR1X_TOR_BIN)")?;
    let nodo = rete.nodo()?;
    let nodo_file = rete.nodo_dei_file()?;
    let scheda = firma_con_la_mia_identita(&chiavi, m4tr1x_protocol::KIND_FILE, testo.trim(), &tags).map_err(|e| format!("scheda non firmata: {e}"))?;
    // l'autorizzazione di UN pezzo: si firma solo quando quel pezzo deve partire (quelli già sul nodo non la chiedono)
    let autorizza = |sha_pezzo: &str| -> Result<String, String> {
        let ora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
        let tag_aut = m4tr1x_protocol::tag_autorizzazione_file(sha_pezzo, ora, ora + 600).map_err(|e| e.to_string())?;
        let aut = firma_con_la_mia_identita(&chiavi, m4tr1x_protocol::KIND_AUTORIZZAZIONE_FILE, "Carico un file", &tag_aut).map_err(|e| format!("autorizzazione non firmata: {e}"))?;
        Ok(m4tr1x_transport::http::autorizzazione(&serde_json::to_string(&aut).map_err(|e| e.to_string())?))
    };
    let op = &op.media;
    if op.in_corso.swap(true, Ordering::SeqCst) {
        return Err("un'altra operazione sui file è in corso".into());
    }
    let esito = async {
        attendi_tor(&tor, op).await?;
        match m4tr1x_feed::carica_file_a_pezzi(&*tor, &nodo_file, &file, autorizza, |f| op.imposta(Avanzamento::from(&f))).await {
            Ok(()) => {}
            Err(m4tr1x_feed::ErroreCarico::Rifiutato { stato, motivo }) => return Err(format!("il nodo dei file non ha accettato il file ({stato}): {motivo}")),
            Err(m4tr1x_feed::ErroreCarico::Autorizzazione(e)) => return Err(e),
            Err(m4tr1x_feed::ErroreCarico::NonRaggiungibile(e)) => return Err(format!("il file non è partito tutto: il nodo dei file non risponde ancora. Riprova tra un minuto, riparte dai pezzi mancanti ({})", e.ultimo)),
        }
        m4tr1x_feed::pubblica(&*tor, &nodo, &scheda, m4tr1x_feed::BUDGET, |f| op.imposta(Avanzamento::from(&f))).await.map_err(|e| match e {
            m4tr1x_feed::ErrorePubblica::Rifiutato(m) => format!("il nodo ha rifiutato la scheda: {m}"),
            m4tr1x_feed::ErrorePubblica::NonRaggiungibile(n) => format!("file caricato, scheda non partita: riprova tra un minuto ({})", n.ultimo),
        })
    }
    .await;
    op.in_corso.store(false, Ordering::SeqCst);
    esito?;
    *scelto.0.lock().unwrap() = None;
    let file = m4tr1x_protocol::file_da_evento(&scheda).ok_or("scheda non leggibile")?;
    Ok(Media { id: scheda.id, autore: scheda.pubkey, nome: None, testo: scheda.content, creato: scheda.created_at, file: file.into() })
}

/// Fase dell'operazione sui file in corso, per l'interfaccia.
#[tauri::command]
fn media_avanzamento(op: tauri::State<'_, Operazioni>) -> Avanzamento {
    op.media.fase()
}

// ─── Alveare: foto e reel a celle esagonali (docs/design/VIDEO_A_CELLE.md) ───
// La codifica e la decodifica stanno QUI, sul computer di chi carica e di chi guarda (crate m4tr1x-celle: aritmetica
// intera, nessuna dipendenza): nessun hub in mezzo ricodifica niente. L'interfaccia manda i pixel grezzi (RGBA dal
// canvas: mai il file, mai il nome: I-16 vale da sé, il formato porta solo pixel) e riceve pixel grezzi. In rete
// vanno i pezzi: il primo è l'anteprima (l'immagine intera in pochi KB), gli altri la affinano; chi guarda vede
// l'immagine dal primo pezzo e la vede migliorare mentre gli altri scendono.

/// Pixel al massimo di una foto o di un fotogramma (l'interfaccia riduce prima: una foto da 2 MP costa ~25 MB di
/// memoria in codifica).
const CELLE_PIXEL_MAX: u64 = 2560 * 2560;
/// Fotogrammi al massimo di un reel (30 s a 20 fotogrammi al secondo): stanno in memoria come pixel grezzi.
const CELLE_FOTOGRAMMI_MAX: u32 = 600;
/// Pixel al massimo di un fotogramma di reel.
const CELLE_PIXEL_FOTOGRAMMA_MAX: u64 = 720 * 720;
/// Una chiave ogni tanti fotogrammi: ogni gruppo è un pezzo che si legge da solo (~4 s a 12 fotogrammi al secondo).
const CELLE_CHIAVE_OGNI: u16 = 48;

fn rgb_da_rgba(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect()
}

fn rgba_da_rgb(rgb: &[u8]) -> Vec<u8> {
    rgb.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect()
}

/// Codifica a celle quello che arriva dall'interfaccia: corpo grezzo = larghezza, altezza, numero di fotogrammi,
/// fotogrammi al secondo (quattro numeri a 32 bit, poco significativo prima), poi i pixel RGBA di ogni fotogramma.
/// Un fotogramma con 0 al secondo è una foto. Il pacchetto resta qui in attesa di `celle_pubblica`; all'interfaccia
/// tornano solo i conti (pezzi, byte, anteprima).
#[tauri::command]
async fn celle_prepara(request: tauri::ipc::Request<'_>, alveare: tauri::State<'_, Alveare>) -> Result<CellePronte, String> {
    let tauri::ipc::InvokeBody::Raw(b) = request.body() else { return Err("servono i pixel".into()) };
    if b.len() < 16 {
        return Err("intestazione dei pixel mancante".into());
    }
    let numero = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let (w, h, n, fps) = (numero(0), numero(4), numero(8), numero(12));
    if w == 0 || h == 0 || n == 0 || w > 65535 || h > 65535 {
        return Err("misure dei pixel non valide".into());
    }
    let pixel = w as u64 * h as u64;
    let foto = fps == 0;
    if foto && (n != 1 || pixel > CELLE_PIXEL_MAX) {
        return Err("una foto è un fotogramma solo, al massimo 2560×2560".into());
    }
    if !foto && (fps > 60 || n > CELLE_FOTOGRAMMI_MAX || pixel > CELLE_PIXEL_FOTOGRAMMA_MAX) {
        return Err(format!("un reel è al massimo {CELLE_FOTOGRAMMI_MAX} fotogrammi da 720×720, fino a 60 al secondo"));
    }
    let attesi = 16 + pixel * 4 * n as u64;
    if b.len() as u64 != attesi {
        return Err("i pixel non corrispondono alle misure".into());
    }
    if alveare.in_corso.swap(true, Ordering::SeqCst) {
        return Err("l'alveare è già al lavoro".into());
    }
    alveare.imposta(Avanzamento { fase: "codifico".into(), secondi: 0, tentativo: 0, pezzi_fatti: 0, pezzi_totali: n });
    let fotogrammi: Vec<m4tr1x_celle::Immagine> = b[16..].chunks_exact((pixel * 4) as usize).map(|f| m4tr1x_celle::Immagine { larghezza: w, altezza: h, rgb: rgb_da_rgba(f) }).collect();
    let fase = alveare.fase.clone();
    let t0 = std::time::Instant::now();
    let esito = tauri::async_runtime::spawn_blocking(move || -> Result<(Vec<Vec<u8>>, &'static str, u32), String> {
        if foto {
            let (c, _) = m4tr1x_celle::codifica(&fotogrammi[0], m4tr1x_celle::Parametri::default()).map_err(|e| e.to_string())?;
            Ok((m4tr1x_celle::pacchetto::pezzi_foto(&c), m4tr1x_protocol::MIME_CELLE_FOTO, 0))
        } else {
            let p = m4tr1x_celle::reel::ParametriReel { chiave_ogni: CELLE_CHIAVE_OGNI, fps: fps as u8, ..m4tr1x_celle::reel::ParametriReel::default() };
            let r = m4tr1x_celle::reel::codifica_reel_con(&fotogrammi, p, |fatti| {
                *fase.lock().unwrap() = Some(Avanzamento { fase: "codifico".into(), secondi: t0.elapsed().as_secs(), tentativo: 0, pezzi_fatti: u32::try_from(fatti).unwrap_or(u32::MAX), pezzi_totali: n });
            })
            .map_err(|e| e.to_string())?;
            Ok((m4tr1x_celle::pacchetto::pezzi_reel(&r), m4tr1x_protocol::MIME_CELLE_REEL, n))
        }
    })
    .await
    .map_err(|e| format!("codifica interrotta: {e}"))
    .and_then(|r| r);
    alveare.in_corso.store(false, Ordering::SeqCst);
    *alveare.fase.lock().unwrap() = None;
    let (pezzi, mime, fotogrammi) = esito?;
    let troppo = pezzi.iter().any(|p| p.len() as u64 > m4tr1x_protocol::PEZZO);
    if troppo || pezzi.len() > m4tr1x_protocol::MASSIMO_PEZZI_CELLE {
        return Err("il contenuto è troppo grande per viaggiare a pezzi: riduci la misura o la durata".into());
    }
    let dimensione = pezzi.iter().map(|p| p.len() as u64).sum();
    let anteprima = pezzi[0].len() as u64;
    let quanti = u32::try_from(pezzi.len()).unwrap_or(u32::MAX);
    *alveare.pronto.lock().unwrap() = Some((pezzi, mime));
    Ok(CellePronte { mime: mime.into(), pezzi: quanti, dimensione, anteprima, fotogrammi, millisecondi: t0.elapsed().as_millis() as u64 })
}

/// Pubblica il pacchetto a celle preparato: ogni pezzo parte come blob (con la sua autorizzazione firmata, quelli
/// già sul nodo si saltano), poi la scheda firmata con hash e misura di ogni pezzo va al nodo.
#[tauri::command]
async fn celle_pubblica(sezione: String, titolo: String, testo: String, rete: tauri::State<'_, Rete>, op: tauri::State<'_, Operazioni>, chiavi: tauri::State<'_, Chiavi>, alveare: tauri::State<'_, Alveare>) -> Result<Media, String> {
    let sez = sezione_da(&sezione)?;
    if testo.chars().count() > m4tr1x_protocol::MASSIMO_TESTO_POST {
        return Err(format!("la didascalia supera i {} caratteri", m4tr1x_protocol::MASSIMO_TESTO_POST));
    }
    let (pezzi, mime) = alveare.pronto.lock().unwrap().clone().ok_or("prepara prima una foto o un reel")?;
    let intero: Vec<u8> = pezzi.concat();
    let sha = m4tr1x_transport::http::sha256_hex(&intero);
    drop(intero);
    let hash: Vec<String> = pezzi.iter().map(|p| m4tr1x_transport::http::sha256_hex(p)).collect();
    let misure: Vec<u64> = pezzi.iter().map(|p| p.len() as u64).collect();
    let dimensione: u64 = misure.iter().sum();
    let tags = m4tr1x_protocol::tag_file_a_celle(sez, &sha, mime, dimensione, &titolo, &hash, &misure).map_err(|e| match e {
        m4tr1x_protocol::Rifiuto::Forma(m) => m.to_string(),
        altro => altro.to_string(),
    })?;
    let tor = rete.tor.clone().ok_or("tor non configurato (M4TR1X_TOR_BIN)")?;
    let nodo = rete.nodo()?;
    let nodo_file = rete.nodo_dei_file()?;
    let scheda = firma_con_la_mia_identita(&chiavi, m4tr1x_protocol::KIND_FILE, testo.trim(), &tags).map_err(|e| format!("scheda non firmata: {e}"))?;
    let autorizza = |sha_pezzo: &str| -> Result<String, String> {
        let ora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
        let tag_aut = m4tr1x_protocol::tag_autorizzazione_file(sha_pezzo, ora, ora + 600).map_err(|e| e.to_string())?;
        let aut = firma_con_la_mia_identita(&chiavi, m4tr1x_protocol::KIND_AUTORIZZAZIONE_FILE, "Carico un file", &tag_aut).map_err(|e| format!("autorizzazione non firmata: {e}"))?;
        Ok(m4tr1x_transport::http::autorizzazione(&serde_json::to_string(&aut).map_err(|e| e.to_string())?))
    };
    let op = &op.media;
    if op.in_corso.swap(true, Ordering::SeqCst) {
        return Err("un'altra operazione sui file è in corso".into());
    }
    let fette: Vec<&[u8]> = pezzi.iter().map(|p| p.as_slice()).collect();
    let esito = async {
        attendi_tor(&tor, op).await?;
        match m4tr1x_feed::carica_pezzi(&*tor, &nodo_file, &fette, autorizza, |f| op.imposta(Avanzamento::from(&f))).await {
            Ok(()) => {}
            Err(m4tr1x_feed::ErroreCarico::Rifiutato { stato, motivo }) => return Err(format!("il nodo dei file non ha accettato un pezzo ({stato}): {motivo}")),
            Err(m4tr1x_feed::ErroreCarico::Autorizzazione(e)) => return Err(e),
            Err(m4tr1x_feed::ErroreCarico::NonRaggiungibile(e)) => return Err(format!("non sono partiti tutti i pezzi: il nodo dei file non risponde ancora. Riprova tra un minuto, riparte dai pezzi mancanti ({})", e.ultimo)),
        }
        m4tr1x_feed::pubblica(&*tor, &nodo, &scheda, m4tr1x_feed::BUDGET, |f| op.imposta(Avanzamento::from(&f))).await.map_err(|e| match e {
            m4tr1x_feed::ErrorePubblica::Rifiutato(m) => format!("il nodo ha rifiutato la scheda: {m}"),
            m4tr1x_feed::ErrorePubblica::NonRaggiungibile(n) => format!("pezzi caricati, scheda non partita: riprova tra un minuto ({})", n.ultimo),
        })
    }
    .await;
    op.in_corso.store(false, Ordering::SeqCst);
    esito?;
    *alveare.pronto.lock().unwrap() = None;
    let file = m4tr1x_protocol::file_da_evento(&scheda).ok_or("scheda non leggibile")?;
    Ok(Media { id: scheda.id, autore: scheda.pubkey, nome: None, testo: scheda.content, creato: scheda.created_at, file: file.into() })
}

/// Decodifica i pezzi arrivati e non ancora usati di `visto` (chiamato da un filo a parte): una foto si rifà da
/// tutti i pezzi che ci sono; di un reel ogni pezzo aggiunge i suoi fotogrammi.
fn decodifica_pezzi_arrivati(visto: &Mutex<Visto>, generazione: u32) -> bool {
    let (mime, pezzi, da) = {
        let v = visto.lock().unwrap();
        if v.generazione != generazione {
            return false;
        }
        if v.decodificati == v.pezzi.len() {
            return true;
        }
        (v.mime.clone(), v.pezzi.clone(), v.decodificati)
    };
    let esito: Result<(Vec<Vec<u8>>, u32, u32, u8, bool), String> = if mime == m4tr1x_protocol::MIME_CELLE_FOTO {
        let viste: Vec<&[u8]> = pezzi.iter().map(|p| p.as_slice()).collect();
        m4tr1x_celle::pacchetto::apri_foto(&viste).map(|img| (vec![rgba_da_rgb(&img.rgb)], img.larghezza, img.altezza, 0, true)).map_err(|e| e.to_string())
    } else {
        let mut nuovi = Vec::new();
        let mut misura = (0, 0, 0);
        let mut r = Ok(());
        for p in &pezzi[da..] {
            match m4tr1x_celle::pacchetto::apri_reel(p) {
                Ok((fotogrammi, fps)) => {
                    for f in fotogrammi {
                        misura = (f.larghezza, f.altezza, fps);
                        nuovi.push(rgba_da_rgb(&f.rgb));
                    }
                }
                Err(e) => {
                    r = Err(e.to_string());
                    break;
                }
            }
        }
        r.map(|_| (nuovi, misura.0, misura.1, misura.2, false))
    };
    let mut v = visto.lock().unwrap();
    if v.generazione != generazione {
        return false;
    }
    match esito {
        Ok((fotogrammi, w, h, fps, sostituisci)) => {
            if sostituisci {
                v.fotogrammi = fotogrammi;
            } else {
                v.fotogrammi.extend(fotogrammi);
            }
            if w > 0 {
                v.larghezza = w;
                v.altezza = h;
            }
            if fps > 0 {
                v.fps = fps;
            }
            v.decodificati = pezzi.len();
            true
        }
        Err(e) => {
            v.errore = Some(format!("il contenuto non si legge: {e}"));
            v.finito = true;
            false
        }
    }
}

/// Apre una foto o un reel a celle dalla sua scheda: i pezzi scendono via Tor uno alla volta (prima dal nodo dei
/// file, poi dal nodo), ognuno accettato solo con hash e misura della scheda (I-17), e si decodificano man mano.
/// Torna subito: l'interfaccia legge `celle_stato` e `celle_quadro` ogni mezzo secondo.
#[tauri::command]
async fn celle_apri(sha256: String, mime: String, dimensione: u64, pezzi: Vec<String>, misure: Vec<u64>, rete: tauri::State<'_, Rete>, alveare: tauri::State<'_, Alveare>) -> Result<(), String> {
    if sha256.len() != 64 || !sha256.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) {
        return Err("hash del file non valido".into());
    }
    if !m4tr1x_protocol::MIME_CELLE.contains(&mime.as_str()) || !m4tr1x_protocol::strati_validi(dimensione, &pezzi, &misure) {
        return Err("scheda a celle non valida".into());
    }
    let tor = rete.tor.clone().ok_or("tor non configurato (M4TR1X_TOR_BIN)")?;
    let fonti = rete.fonti()?;
    if alveare.in_corso.swap(true, Ordering::SeqCst) {
        return Err("l'alveare è già al lavoro".into());
    }
    let generazione = {
        let mut v = alveare.visto.lock().unwrap();
        let g = v.generazione.wrapping_add(1);
        *v = Visto { generazione: g, mime: mime.clone(), pezzi_totali: u32::try_from(pezzi.len()).unwrap_or(u32::MAX), ..Visto::default() };
        g
    };
    let attesi: Vec<(String, usize)> = pezzi.iter().cloned().zip(misure.iter().map(|&m| usize::try_from(m).unwrap_or(usize::MAX))).collect();
    let visto = alveare.visto.clone();
    let fase = alveare.fase.clone();
    let in_corso = alveare.in_corso.clone();
    // filo che decodifica i pezzi man mano che arrivano
    let visto_dec = visto.clone();
    let decodifica = tauri::async_runtime::spawn_blocking(move || loop {
        if !decodifica_pezzi_arrivati(&visto_dec, generazione) {
            break;
        }
        let v = visto_dec.lock().unwrap();
        if v.generazione != generazione || (v.finito && v.decodificati == v.pezzi.len()) {
            break;
        }
        drop(v);
        std::thread::sleep(std::time::Duration::from_millis(50));
    });
    tauri::async_runtime::spawn(async move {
        let op = Operazione { in_corso: AtomicBool::new(true), fase: Mutex::new(None) };
        let esito = async {
            *fase.lock().unwrap() = Some(Avanzamento::tor(0));
            attendi_tor(&tor, &op).await?;
            let avanza = |f: m4tr1x_feed::Fase| *fase.lock().unwrap() = Some(Avanzamento::from(&f));
            m4tr1x_feed::scarica_pezzi(&*tor, &fonti, &attesi, &avanza, |_, b| {
                let mut v = visto.lock().unwrap();
                if v.generazione == generazione {
                    v.byte_arrivati += b.len() as u64;
                    v.pezzi.push(b.to_vec());
                }
            })
            .await
            .map_err(|e| match e {
                m4tr1x_feed::ErroreScarico::Scheda => "scheda del file non valida".to_string(),
                m4tr1x_feed::ErroreScarico::NessunaFonte { pezzo, ultimo } => format!("nessun nodo ha dato il pezzo {pezzo} ({ultimo})"),
                m4tr1x_feed::ErroreScarico::NonCorrisponde => "un pezzo ricevuto non è quello della scheda: scartato".to_string(),
            })
        }
        .await;
        {
            let mut v = visto.lock().unwrap();
            if v.generazione == generazione {
                if let Err(e) = esito {
                    // quello che è sceso resta visibile: l'anteprima vale anche a metà
                    v.errore = Some(e);
                }
                v.finito = true;
            }
        }
        let _ = decodifica.await;
        *fase.lock().unwrap() = None;
        in_corso.store(false, Ordering::SeqCst);
    });
    Ok(())
}

/// Dove sta la foto o il reel che si sta guardando.
#[tauri::command]
fn celle_stato(alveare: tauri::State<'_, Alveare>) -> CelleStato {
    let v = alveare.visto.lock().unwrap();
    CelleStato {
        larghezza: v.larghezza,
        altezza: v.altezza,
        pezzi_arrivati: u32::try_from(v.pezzi.len()).unwrap_or(u32::MAX),
        pezzi_totali: v.pezzi_totali,
        byte_arrivati: v.byte_arrivati,
        fotogrammi: u32::try_from(v.fotogrammi.len()).unwrap_or(u32::MAX),
        fps: v.fps,
        finito: v.finito && (v.decodificati == v.pezzi.len() || v.errore.is_some()),
        errore: v.errore.clone(),
    }
}

/// I pixel RGBA del fotogramma `indice` di quello che si sta guardando (0 per una foto), grezzi, preceduti da
/// larghezza e altezza (32 bit, poco significativo prima). Vuoto se quel fotogramma non c'è ancora.
#[tauri::command]
fn celle_quadro(indice: u32, alveare: tauri::State<'_, Alveare>) -> tauri::ipc::Response {
    let v = alveare.visto.lock().unwrap();
    let mut b = Vec::new();
    if let Some(f) = v.fotogrammi.get(indice as usize) {
        b.reserve(8 + f.len());
        b.extend_from_slice(&v.larghezza.to_le_bytes());
        b.extend_from_slice(&v.altezza.to_le_bytes());
        b.extend_from_slice(f);
    }
    tauri::ipc::Response::new(b)
}

/// Fase della codifica o dello scarico a celle in corso.
#[tauri::command]
fn celle_avanzamento(alveare: tauri::State<'_, Alveare>) -> Avanzamento {
    alveare.fase.lock().unwrap().clone().unwrap_or_else(Avanzamento::fermo)
}

// ─── Forum ───────────────────────────────────────────────────────────────────
// Stesso schema della v2 (discussioni e risposte sono kind 1 con il tag del forum). Tutto ciò che arriva dal
// nodo è verificato in m4tr1x-feed; si firma con la tua identità da `firma_con_la_mia_identita`, come i post.

/// Testo di una discussione o di una risposta: non vuoto, entro il limite dei post.
fn testo_forum(testo: &str) -> Result<String, String> {
    let t = testo.trim().to_string();
    if t.is_empty() {
        return Err("il testo è vuoto".into());
    }
    if t.chars().count() > m4tr1x_protocol::MASSIMO_TESTO_POST {
        return Err(format!("il testo supera i {} caratteri", m4tr1x_protocol::MASSIMO_TESTO_POST));
    }
    Ok(t)
}

/// Pubblica sul nodo un evento GIÀ firmato, con la fase del forum. Una operazione del forum alla volta.
async fn pubblica_nel_forum(rete: &Rete, op: &Operazione, evento: &m4tr1x_protocol::Evento) -> Result<(), String> {
    let tor = rete.tor.clone().ok_or("tor non configurato (M4TR1X_TOR_BIN)")?;
    let nodo = rete.nodo()?;
    if op.in_corso.swap(true, Ordering::SeqCst) {
        return Err("il forum è già in aggiornamento".into());
    }
    let esito = async {
        attendi_tor(&tor, op).await?;
        m4tr1x_feed::pubblica(&*tor, &nodo, evento, m4tr1x_feed::BUDGET, |f| op.imposta(Avanzamento::from(&f))).await.map(|_| ()).map_err(|e| match e {
            m4tr1x_feed::ErrorePubblica::Rifiutato(m) => format!("il nodo ha rifiutato: {m}"),
            m4tr1x_feed::ErrorePubblica::NonRaggiungibile(n) => format!("non è partito: il nodo non risponde ancora. Il testo resta qui, riprova tra un minuto ({})", n.ultimo),
        })
    }
    .await;
    op.in_corso.store(false, Ordering::SeqCst);
    esito
}

/// Le discussioni del forum dal nodo, verificate nel lato Rust.
#[tauri::command]
async fn forum(rete: tauri::State<'_, Rete>, op: tauri::State<'_, Operazioni>) -> Result<Forum, String> {
    let tor = rete.tor.clone().ok_or("tor non configurato (M4TR1X_TOR_BIN)")?;
    let nodo = rete.nodo()?;
    let op = &op.forum;
    if op.in_corso.swap(true, Ordering::SeqCst) {
        return Err("il forum è già in aggiornamento".into());
    }
    let esito = async {
        attendi_tor(&tor, op).await?;
        m4tr1x_feed::leggi_forum(&*tor, &nodo, m4tr1x_protocol::Rete::Prova, m4tr1x_feed::BUDGET, |f| op.imposta(Avanzamento::from(&f)))
            .await
            .map(Forum::from)
            .map_err(|e| format!("il nodo non risponde ancora: Tor continua a cercarlo. Riprova tra un minuto ({})", e.ultimo))
    }
    .await;
    op.in_corso.store(false, Ordering::SeqCst);
    esito
}

/// Una discussione con le sue risposte. Ok(None): il nodo non ce l'ha (o l'id non è di una discussione).
#[tauri::command]
async fn forum_discussione(id: String, rete: tauri::State<'_, Rete>, op: tauri::State<'_, Operazioni>) -> Result<Option<DiscussioneAperta>, String> {
    if id.len() != 64 || !id.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) {
        return Err("id della discussione non valido".into());
    }
    let tor = rete.tor.clone().ok_or("tor non configurato (M4TR1X_TOR_BIN)")?;
    let nodo = rete.nodo()?;
    let op = &op.forum;
    if op.in_corso.swap(true, Ordering::SeqCst) {
        return Err("il forum è già in aggiornamento".into());
    }
    let esito = async {
        attendi_tor(&tor, op).await?;
        m4tr1x_feed::leggi_discussione(&*tor, &nodo, m4tr1x_protocol::Rete::Prova, &id, m4tr1x_feed::BUDGET, |f| op.imposta(Avanzamento::from(&f)))
            .await
            .map(|d| d.map(DiscussioneAperta::from))
            .map_err(|e| format!("il nodo non risponde ancora: Tor continua a cercarlo. Riprova tra un minuto ({})", e.ultimo))
    }
    .await;
    op.in_corso.store(false, Ordering::SeqCst);
    esito
}

/// Apre una discussione nuova (titolo e testo), firmata con la TUA identità.
#[tauri::command]
async fn forum_pubblica(titolo: String, testo: String, rete: tauri::State<'_, Rete>, op: tauri::State<'_, Operazioni>, chiavi: tauri::State<'_, Chiavi>) -> Result<Discussione, String> {
    let testo = testo_forum(&testo)?;
    let tags = m4tr1x_protocol::tag_discussione(&titolo).map_err(|e| match e {
        m4tr1x_protocol::Rifiuto::Forma(m) => m.to_string(),
        altro => altro.to_string(),
    })?;
    let evento = firma_con_la_mia_identita(&chiavi, 1, &testo, &tags).map_err(|e| format!("discussione non firmata: {e}"))?;
    pubblica_nel_forum(&rete, &op.forum, &evento).await?;
    let titolo = m4tr1x_protocol::discussione_da_evento(&evento).unwrap_or_default();
    Ok(Discussione { id: evento.id, autore: evento.pubkey, nome: None, titolo, testo: evento.content, creato: evento.created_at, risposte: 0, attivita: evento.created_at })
}

/// Risponde alla discussione `id`, con la TUA identità.
#[tauri::command]
async fn forum_rispondi(id: String, testo: String, rete: tauri::State<'_, Rete>, op: tauri::State<'_, Operazioni>, chiavi: tauri::State<'_, Chiavi>) -> Result<Post, String> {
    let testo = testo_forum(&testo)?;
    let tags = m4tr1x_protocol::tag_risposta(&id).map_err(|e| e.to_string())?;
    let evento = firma_con_la_mia_identita(&chiavi, 1, &testo, &tags).map_err(|e| format!("risposta non firmata: {e}"))?;
    pubblica_nel_forum(&rete, &op.forum, &evento).await?;
    Ok(Post { id: evento.id, autore: evento.pubkey, creato: evento.created_at, testo: evento.content, nome: None })
}

/// Fase del forum in corso, per l'interfaccia.
#[tauri::command]
fn forum_avanzamento(op: tauri::State<'_, Operazioni>) -> Avanzamento {
    op.forum.fase()
}

/// Errore della posta per l'interfaccia: cosa non va, mai l'indirizzo del nodo.
fn errore_posta(e: m4tr1x_posta::ErrorePosta) -> String {
    match e {
        m4tr1x_posta::ErrorePosta::NonRaggiungibile(n) => format!("il nodo non risponde ancora: Tor continua a cercarlo. Riprova tra un minuto ({})", n.ultimo),
        altro => altro.to_string(),
    }
}

/// I TUOI messaggi privati: buste prese dal nodo via Tor, aperte e controllate nel lato Rust (m4tr1x-posta).
/// La chiave dei messaggi si legge dal portachiavi solo dentro m4tr1x-posta, per il tempo di aprire le
/// buste: qui non passa e all'interfaccia arrivano solo testi e chiavi PUBBLICHE.
#[tauri::command]
async fn messaggi(rete: tauri::State<'_, Rete>, op: tauri::State<'_, Operazioni>, chiavi: tauri::State<'_, Chiavi>) -> Result<Posta, String> {
    let tor = rete.tor.clone().ok_or("tor non configurato (M4TR1X_TOR_BIN)")?;
    let nodo = rete.nodo()?;
    let portachiavi = chiavi.0.clone();
    let op = &op.posta;
    if op.in_corso.swap(true, Ordering::SeqCst) {
        return Err("la posta è già in aggiornamento".into());
    }
    let esito = async {
        attendi_tor(&tor, op).await?;
        m4tr1x_posta::leggi(&*tor, &nodo, m4tr1x_protocol::Rete::Prova, &*portachiavi, m4tr1x_feed::BUDGET, |f| op.imposta(Avanzamento::from(&f))).await.map(Posta::from).map_err(errore_posta)
    }
    .await;
    op.in_corso.store(false, Ordering::SeqCst);
    esito
}

/// Manda un messaggio privato alla persona con chiave pubblica d'identità `a`. Al nodo arrivano solo buste
/// cifrate (kind 1059): né il testo né chi scrive. Se non parte, il testo resta nella casella.
#[tauri::command]
async fn messaggio_invia(a: String, testo: String, rete: tauri::State<'_, Rete>, op: tauri::State<'_, Operazioni>, chiavi: tauri::State<'_, Chiavi>) -> Result<Messaggio, String> {
    if testo.trim().is_empty() {
        return Err("il messaggio è vuoto".into());
    }
    let tor = rete.tor.clone().ok_or("tor non configurato (M4TR1X_TOR_BIN)")?;
    let nodo = rete.nodo()?;
    let portachiavi = chiavi.0.clone();
    let op = &op.posta;
    if op.in_corso.swap(true, Ordering::SeqCst) {
        return Err("la posta è già in aggiornamento".into());
    }
    let esito = async {
        attendi_tor(&tor, op).await?;
        m4tr1x_posta::invia(&*tor, &nodo, m4tr1x_protocol::Rete::Prova, &*portachiavi, a.trim(), &testo, m4tr1x_feed::BUDGET, |f| op.imposta(Avanzamento::from(&f)))
            .await
            .map(Messaggio::from)
            .map_err(errore_posta)
    }
    .await;
    op.in_corso.store(false, Ordering::SeqCst);
    esito
}

/// Fase della posta in corso (lettura o invio), per l'interfaccia.
#[tauri::command]
fn messaggi_avanzamento(op: tauri::State<'_, Operazioni>) -> Avanzamento {
    op.posta.fase()
}

/// Fase della pubblicazione in corso (o dell'ultima), per l'interfaccia.
#[tauri::command]
fn pubblica_avanzamento(op: tauri::State<'_, Operazioni>) -> Avanzamento {
    op.post.fase()
}

/// Identità salvata (solo chiavi pubbliche), se c'è.
#[tauri::command]
fn identita(chiavi: tauri::State<'_, Chiavi>) -> Result<Option<Identita>, String> {
    creazione::identita_salvata(&*chiavi.0).map(|o| o.map(Identita::from)).map_err(|e| e.to_string())
}

/// Apre la finestra del seme: invisibile, protetta dalle catture, poi mostrata.
/// `modo`: "crea" o "recupera". `async`: su Windows creare una finestra da un comando
/// sincrono blocca il thread che dovrebbe disegnarla (documentazione di Tauri 2).
#[tauri::command]
async fn apri_finestra_seme<R: Runtime>(app: tauri::AppHandle<R>, modo: String) -> Result<(), String> {
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
fn seme_nuovo<R: Runtime>(window: WebviewWindow<R>, stato: tauri::State<'_, StatoSeme>, rivedi: bool) -> Result<SemeDaMostrare, String> {
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
fn seme_conferma<R: Runtime>(window: WebviewWindow<R>, stato: tauri::State<'_, StatoSeme>, chiavi: tauri::State<'_, Chiavi>, mut risposte: Vec<String>) -> Result<Identita, String> {
    solo_finestra_seme(&window)?;
    let mut g = stato.0.lock().unwrap();
    let esito = g.as_ref().ok_or("nessuna creazione in corso")?.conferma(&risposte, &*chiavi.0);
    risposte.iter_mut().for_each(|r| r.zeroize());
    let id = esito.map_err(|e| e.to_string())?;
    *g = None; // parole cancellate dalla memoria
    drop(g);
    let _ = window.close();
    Ok(id.into())
}

/// Recupero dalle 24 parole. Solo finestra del seme.
#[tauri::command]
fn seme_recupera<R: Runtime>(window: WebviewWindow<R>, chiavi: tauri::State<'_, Chiavi>, mut parole: String) -> Result<Identita, String> {
    solo_finestra_seme(&window)?;
    let esito = creazione::recupera(&parole, &*chiavi.0);
    parole.zeroize();
    let id = esito.map_err(|e| e.to_string())?;
    let _ = window.close();
    Ok(id.into())
}

/// Identificatore dell'app (uguale a `identifier` in tauri.conf.json: lo controlla tests/unit/crash-log.test.js).
const IDENTIFICATORE: &str = "org.m4tr1x.app";

/// Stato, comandi e finestre dell'app su un `Builder` qualsiasi: `run` lo usa con il runtime vero e il
/// portachiavi del sistema, la prova dinamica di I-31 con il runtime finto di Tauri e un portachiavi in memoria.
/// Il registro dei crash NON sta qui: lo installa `run`, prima di tutto, e `prima` lo riallinea alla
/// cartella dati. Un solo `setup`: in Tauri un secondo `setup` sostituirebbe il primo (e tor non partirebbe).
pub fn costruisci<R: Runtime>(
    b: Builder<R>,
    chiavi: Arc<dyn Portachiavi>,
    prima: impl FnOnce(&mut tauri::App<R>) -> Result<(), Box<dyn std::error::Error>> + Send + 'static,
) -> Builder<R> {
    b.manage(StatoSeme::default())
        .manage(Operazioni::default())
        .manage(FileScelto::default())
        .manage(Alveare::default())
        .manage(Chiavi(chiavi))
        .setup(move |app| {
            prima(app)?;
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
        .invoke_handler(tauri::generate_handler![stato, identita, apri_finestra_seme, seme_nuovo, seme_conferma, seme_recupera, feed, feed_avanzamento, pubblica, pubblica_avanzamento, profilo_mio, profilo_salva, profilo_avanzamento, messaggi, messaggio_invia, messaggi_avanzamento, nodo_impostato, nodo_imposta, nodo_file_impostato, nodo_file_imposta, forum, forum_discussione, forum_pubblica, forum_rispondi, forum_avanzamento, media, media_file, media_prepara, media_pubblica, media_avanzamento, celle_prepara, celle_pubblica, celle_apri, celle_stato, celle_quadro, celle_avanzamento])
}

pub fn run() {
    // PRIMA di tutto: un crash dell'avvio (anche dentro Tauri) deve finire nel registro (m4tr1x-crash)
    let cartella_crash = m4tr1x_crash::cartella_predefinita(IDENTIFICATORE);
    if let Some(c) = cartella_crash.clone() {
        m4tr1x_crash::installa(c, env!("CARGO_PKG_VERSION"));
    }
    costruisci(tauri::Builder::default(), Arc::new(Sistema), move |app| {
        // se Tauri usa un'altra cartella dati, il registro la segue
        let dati = app.path().app_data_dir()?;
        if cartella_crash.as_deref() != Some(dati.as_path()) {
            m4tr1x_crash::installa(dati, env!("CARGO_PKG_VERSION"));
        }
        Ok(())
    })
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
