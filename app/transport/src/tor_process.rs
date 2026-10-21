//! `TorProcessTransport` — tutto il desktop: Windows, Linux (macOS con il suo lock) (ADR-0016
//! aggiornata il 21/10/2026, I-37).
//!
//! L'app avvia il `tor` classico incluso nel pacchetto e ci parla via SOCKS5 e
//! porta di controllo. Cosa garantisce (I-37, corretta il 16/10/2026):
//!   - binario (e su Linux le sue librerie) con hash SHA-256 fissato, controllato PRIMA di avviarlo:
//!     ogni file si legge UNA volta in memoria, si verifica lì, si scrive in una cartella dell'app
//!     solo-proprietario e tor parte da quella copia (niente sostituzioni tra verifica e avvio,
//!     21/10/2026); su Linux con l'ambiente pulito (niente LD_PRELOAD ereditati);
//!   - SOCKS e controllo solo su 127.0.0.1, porte scelte da tor all'avvio (`auto`);
//!   - SOCKS con `OnionTrafficOnly`: chiunque lo usi raggiunge solo onion, mai host in chiaro;
//!   - credenziali SOCKS casuali per sessione + `IsolateSOCKSAuth`: senza le nostre
//!     credenziali nessuno condivide i nostri circuiti (NON impedisce di collegarsi:
//!     tor C non ha un'opzione per renderle obbligatorie);
//!   - porta di controllo con cookie; `TAKEOWNERSHIP` + `__OwningControllerProcess`:
//!     tor si chiude quando l'app chiude la connessione di controllo o muore.
//!   - nessun servizio onion ospitato: l'app è solo client.

use crate::{BootstrapInfo, Isolation, OnionV3, Result, Stream, Transport, TransportError, TransportStatus};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};

/// Configurazione: da dove viene tor, che hash devono avere lui e le sue librerie, dove tiene i dati.
#[derive(Clone, Debug)]
pub struct TorProcessConfig {
    pub tor_bin: PathBuf,
    /// SHA-256 atteso del binario, esadecimale minuscolo (dal lock della piattaforma).
    pub tor_sha256: String,
    /// Librerie che tor carica, cercate ACCANTO a `tor_bin`: (nome del file, SHA-256 atteso). Linux: libcrypto,
    /// libssl, libevent del bundle (tor non ha RPATH: le trova solo con LD_LIBRARY_PATH). Windows: nessuna.
    pub librerie: Vec<(String, String)>,
    /// Cartella dati dell'app: tor usa `<data_dir>/tor`, mai condivisa con altri Tor; la copia verificata
    /// dei binari sta in `<data_dir>/tor-bin`.
    pub data_dir: PathBuf,
    /// Tempo massimo per arrivare al 100% del bootstrap.
    pub bootstrap_timeout: Duration,
}

/// Lock del tor della piattaforma, incluso nel programma alla COMPILAZIONE: gli hash attesi non si leggono
/// da disco mentre l'app gira, quindi non si possono cambiare accanto all'app. macOS: lock da aggiungere,
/// fino ad allora il trasporto resta spento.
#[cfg(windows)]
const LOCK_TOR: Option<&str> = Some(include_str!("../../tor-windows.lock.json"));
#[cfg(target_os = "linux")]
const LOCK_TOR: Option<&str> = Some(include_str!("../../tor-linux.lock.json"));
#[cfg(not(any(windows, target_os = "linux")))]
const LOCK_TOR: Option<&str> = None;

/// Nome del file di lock della piattaforma (per le prove), se c'è.
pub const FILE_LOCK: Option<&str> = if cfg!(windows) {
    Some("tor-windows.lock.json")
} else if cfg!(target_os = "linux") {
    Some("tor-linux.lock.json")
} else {
    None
};

/// Hash fissati da un lock: (sha256 di tor, librerie). Panico se il lock è malformato: è nel programma,
/// un lock rotto è un errore di build, non di chi usa l'app.
pub fn leggi_lock(json: &str) -> (String, Vec<(String, String)>) {
    let v: serde_json::Value = serde_json::from_str(json).expect("lock di tor non valido");
    let hash = |h: &str| {
        assert!(h.len() == 64 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "{h} non è un SHA-256");
        h.to_owned()
    };
    let tor = hash(v["tor_sha256"].as_str().expect("tor_sha256 mancante nel lock"));
    let librerie = v["librerie"]
        .as_object()
        .expect("librerie mancanti nel lock")
        .iter()
        .map(|(nome, h)| {
            assert!(!nome.is_empty() && !nome.contains(['/', '\\']) && nome != "." && nome != "..", "nome di libreria non valido: {nome}");
            (nome.clone(), hash(h.as_str().expect("hash della libreria non è testo")))
        })
        .collect();
    (tor, librerie)
}

/// Gli hash del lock della piattaforma, se c'è.
pub fn lock_tor() -> Option<(String, Vec<(String, String)>)> {
    LOCK_TOR.map(leggi_lock)
}

impl TorProcessConfig {
    /// Configurazione dell'APP: tor da `tor_bin` (in F4 la variabile `M4TR1X_TOR_BIN`; dall'installer in F8),
    /// hash SOLO dal lock della piattaforma, cartella dati dell'app, 180 s per il bootstrap (come il nodo).
    /// `None` se tor non è indicato o la piattaforma non ha ancora un lock: il trasporto resta spento.
    pub fn per_l_app(tor_bin: Option<PathBuf>, data_dir: PathBuf) -> Option<Self> {
        let (tor_sha256, librerie) = lock_tor()?;
        Some(Self {
            tor_bin: tor_bin.filter(|p| !p.as_os_str().is_empty())?,
            tor_sha256,
            librerie,
            data_dir,
            bootstrap_timeout: Duration::from_secs(180),
        })
    }
}

/// Cartella con la copia verificata di tor (e librerie) da cui l'app lo avvia.
pub fn cartella_binari(data_dir: &Path) -> PathBuf {
    data_dir.join("tor-bin")
}

/// Un collegamento da NON attraversare: link simbolico, e su Windows qualsiasi punto di analisi
/// (junction, link, segnaposto del cloud). Se ne cancella solo il collegamento, mai il contenuto.
fn e_collegamento(m: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        m.file_type().is_symlink() || m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        m.file_type().is_symlink()
    }
}

/// Cancella `p` senza mai seguire link o junction: un collegamento si toglie da solo (il suo bersaglio
/// resta com'è), una cartella vera si svuota voce per voce con la stessa regola.
pub fn rimuovi_senza_seguire(p: &Path) -> std::io::Result<()> {
    let m = match std::fs::symlink_metadata(p) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if e_collegamento(&m) {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
            // junction e link a cartella: remove_dir toglie il collegamento, non entra nel bersaglio
            return if m.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0 { std::fs::remove_dir(p) } else { std::fs::remove_file(p) };
        }
        #[cfg(not(windows))]
        return std::fs::remove_file(p);
    }
    if m.is_dir() {
        for voce in std::fs::read_dir(p)? {
            rimuovi_senza_seguire(&voce?.path())?;
        }
        std::fs::remove_dir(p)
    } else {
        std::fs::remove_file(p)
    }
}

/// Crea la cartella (che NON deve esistere) leggibile e scrivibile solo dal proprietario. Linux: 0700.
/// Windows: ACL esplicita e PROTETTA (nessuna ereditarietà dalla cartella sopra), un solo accesso
/// completo per l'utente che esegue l'app, ereditato da file e sottocartelle.
fn crea_cartella_solo_proprietario(p: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(p)
    }
    #[cfg(windows)]
    {
        acl_windows::crea_cartella(p)
    }
}

#[cfg(windows)]
mod acl_windows {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::{ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER};
    use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// SID dell'utente che esegue l'app, come testo (S-1-5-21-…).
    pub fn sid_utente() -> std::io::Result<String> {
        // SAFETY: chiamate Win32 con buffer della dimensione chiesta dal sistema; handle chiuso, memoria liberata
        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            let mut n = 0u32;
            GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut n);
            let mut buf = vec![0u8; n as usize];
            let ok = GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), n, &mut n);
            CloseHandle(token);
            if ok == 0 {
                return Err(std::io::Error::last_os_error());
            }
            let utente = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut s: *mut u16 = std::ptr::null_mut();
            if ConvertSidToStringSidW(utente.User.Sid, &mut s) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            let len = (0..).take_while(|&i| *s.add(i) != 0).count();
            let testo = String::from_utf16_lossy(std::slice::from_raw_parts(s, len));
            LocalFree(s.cast());
            Ok(testo)
        }
    }

    pub fn crea_cartella(p: &Path) -> std::io::Result<()> {
        // D:P = DACL protetta (niente ereditato da sopra); OICI = vale anche per file e sottocartelle; FA = tutto.
        // P è una difesa doppia: con una DACL esplicita Windows non eredita comunque, nemmeno se dopo si
        // aggiunge un permesso ereditabile alla cartella sopra (misurato il 21/10/2026). Ciò che conta è
        // passare il descrittore: senza, la cartella erediterebbe l'ACL del profilo.
        let sddl = wide(std::ffi::OsStr::new(&format!("D:P(A;OICI;FA;;;{})", sid_utente()?)));
        // SAFETY: descrittore creato dal sistema, usato per una sola CreateDirectoryW, poi liberato
        unsafe {
            let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, std::ptr::null_mut()) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            let sa = SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: sd, bInheritHandle: 0 };
            let ok = CreateDirectoryW(wide(p.as_os_str()).as_ptr(), &sa);
            let err = std::io::Error::last_os_error();
            LocalFree(sd);
            if ok == 0 {
                return Err(err);
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
pub use acl_windows::sid_utente;

/// Prepara la copia da cui tor parte (I-37, 21/10/2026, contro le sostituzioni tra verifica e avvio):
///  1. legge tor e ogni libreria UNA volta in memoria e ne verifica l'hash sui byte letti — se uno è
///     sbagliato o manca, si ferma qui senza aver toccato il disco;
///  2. cancella la vecchia `tor-bin` senza seguire link o junction;
///  3. la ricrea solo-proprietario e ci scrive i byte VERIFICATI con "crea nuovo" (fallisce se qualcosa
///     esiste già: nessun collegamento preparato da altri viene seguito).
///
/// Restituisce il percorso del tor copiato. Chi gira con lo STESSO utente può comunque toccare la copia:
/// fuori dal modello delle minacce (un programma così ha già in mano l'utente, I-37).
pub fn prepara_binari(cfg: &TorProcessConfig) -> Result<PathBuf> {
    let sorgente = cfg.tor_bin.parent().ok_or_else(|| err_avvio("percorso di tor senza cartella"))?;
    let nome_tor = cfg.tor_bin.file_name().ok_or_else(|| err_avvio("percorso di tor senza nome"))?.to_os_string();
    let mut file = vec![(nome_tor.clone(), cfg.tor_bin.clone(), cfg.tor_sha256.to_ascii_lowercase(), true)];
    for (nome, h) in &cfg.librerie {
        file.push((nome.into(), sorgente.join(nome), h.to_ascii_lowercase(), false));
    }
    // 1. lettura unica e verifica in memoria
    let mut verificati = Vec::with_capacity(file.len());
    for (nome, da, atteso, eseguibile) in file {
        let byte = std::fs::read(&da).map_err(|e| err_avvio(format!("{} non leggibile: {e}", nome.to_string_lossy())))?;
        if hex(&Sha256::digest(&byte)) != atteso {
            return Err(err_avvio(format!("hash di {} diverso da quello fissato", nome.to_string_lossy())));
        }
        verificati.push((nome, byte, eseguibile));
    }
    // 2–3. cartella nuova solo-proprietario, byte verificati scritti lì
    let dest = cartella_binari(&cfg.data_dir);
    std::fs::create_dir_all(&cfg.data_dir).map_err(err_avvio)?;
    rimuovi_senza_seguire(&dest).map_err(|e| err_avvio(format!("vecchia copia di tor non cancellabile: {e}")))?;
    crea_cartella_solo_proprietario(&dest).map_err(|e| err_avvio(format!("cartella dei binari di tor: {e}")))?;
    for (nome, byte, eseguibile) in verificati {
        let mut o = std::fs::OpenOptions::new();
        o.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(if eseguibile { 0o500 } else { 0o400 });
        }
        #[cfg(not(unix))]
        let _ = eseguibile;
        use std::io::Write;
        o.open(dest.join(&nome)).and_then(|mut f| f.write_all(&byte)).map_err(|e| err_avvio(format!("copia di {}: {e}", nome.to_string_lossy())))?;
    }
    Ok(dest.join(nome_tor))
}

struct Running {
    child: Child,
    control: BufReader<TcpStream>,
    socks_port: u16,
}

pub struct TorProcessTransport {
    cfg: TorProcessConfig,
    session_secret: String,
    running: tokio::sync::Mutex<Option<Running>>,
    status: Mutex<TransportStatus>,
}

fn err_avvio<E: std::fmt::Display>(e: E) -> TransportError {
    TransportError::Avvio(e.to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Hash del file su disco (letto per intero: tor.exe sono pochi MB).
pub fn sha256_file(p: &Path) -> std::io::Result<String> {
    Ok(hex(&Sha256::digest(std::fs::read(p)?)))
}

fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    getrandom::fill(&mut b).expect("generatore casuale del sistema non disponibile");
    hex(&b)
}

/// Una risposta della porta di controllo: righe "250-..." fino a "250 ...". Codice + righe.
async fn read_reply(r: &mut BufReader<TcpStream>) -> Result<(u16, Vec<String>)> {
    let mut lines = Vec::new();
    loop {
        let mut line = String::new();
        let n = r.read_line(&mut line).await.map_err(err_avvio)?;
        if n == 0 {
            return Err(err_avvio("porta di controllo chiusa"));
        }
        let line = line.trim_end().to_string();
        if line.len() < 4 {
            return Err(err_avvio("risposta di controllo malformata"));
        }
        let code: u16 = line[..3].parse().map_err(err_avvio)?;
        let sep = line.as_bytes()[3];
        lines.push(line[4..].to_string());
        if sep == b' ' {
            return Ok((code, lines));
        }
    }
}

async fn command(r: &mut BufReader<TcpStream>, cmd: &str) -> Result<Vec<String>> {
    r.get_mut().write_all(format!("{cmd}\r\n").as_bytes()).await.map_err(err_avvio)?;
    let (code, lines) = read_reply(r).await?;
    if code != 250 {
        // mai il comando intero nell'errore: AUTHENTICATE contiene il cookie
        let verbo = cmd.split_whitespace().next().unwrap_or("");
        return Err(err_avvio(format!("{verbo}: {code} {}", lines.join(" "))));
    }
    Ok(lines)
}

/// "PORT=127.0.0.1:12345" scritto da tor con ControlPortWriteToFile.
fn parse_port_file(s: &str) -> Option<u16> {
    let v = s.lines().find_map(|l| l.trim().strip_prefix("PORT="))?;
    let (host, port) = v.rsplit_once(':')?;
    if host != "127.0.0.1" {
        return None; // qualsiasi altra interfaccia è un errore di configurazione
    }
    port.parse().ok()
}

impl TorProcessTransport {
    pub fn new(cfg: TorProcessConfig) -> Self {
        Self {
            cfg,
            session_secret: random_hex(16),
            running: tokio::sync::Mutex::new(None),
            status: Mutex::new(TransportStatus::Spento),
        }
    }

    fn set_status(&self, s: TransportStatus) {
        *self.status.lock().unwrap() = s;
    }

    /// Porta SOCKS (solo per i test di I-37).
    pub async fn socks_port(&self) -> Option<u16> {
        self.running.lock().await.as_ref().map(|r| r.socks_port)
    }

    /// PID del processo tor (solo per i test di I-37).
    pub async fn tor_pid(&self) -> Option<u32> {
        self.running.lock().await.as_ref().and_then(|r| r.child.id())
    }

    /// Porta di controllo e cookie (solo per i test di I-37).
    pub fn control_port_file(&self) -> PathBuf {
        self.cfg.data_dir.join("tor").join("control.port")
    }

    async fn start(&self) -> Result<Running> {
        // 1. hash di tor e librerie verificati sui byte letti, copia solo-proprietario: si avvia QUELLA
        let tor = prepara_binari(&self.cfg)?;
        let dir = self.cfg.data_dir.join("tor");
        std::fs::create_dir_all(&dir).map_err(err_avvio)?;
        let port_file = dir.join("control.port");
        let cookie_file = dir.join("control.cookie");
        let _ = std::fs::remove_file(&port_file);

        let mut cmd = Command::new(&tor);
        #[cfg(target_os = "linux")]
        {
            // ambiente pulito: niente LD_PRELOAD/LD_AUDIT ereditati dentro un tor verificato; le librerie
            // si cercano SOLO nella copia verificata (poi quelle di base del sistema: libc, libz, libm)
            cmd.env_clear().env("LD_LIBRARY_PATH", tor.parent().expect("tor è dentro tor-bin"));
        }
        cmd.arg("--ignore-missing-torrc")
            // nessun torrc di sistema: solo questi argomenti (i due file indicati non esistono)
            .args(["-f", &dir.join("nessun-torrc").to_string_lossy()])
            .args(["--defaults-torrc", &dir.join("nessun-torrc-defaults").to_string_lossy()])
            .args(["--DataDirectory", &dir.join("data").to_string_lossy()])
            // ExtendedErrors (decisione del fondatore, 20/10/2026): sugli onion tor dice PERCHÉ fallisce
            // (descrittore non trovato, introduzione, rendezvous…) invece del generico "6". Non allarga niente.
            .args(["--SocksPort", "127.0.0.1:auto OnionTrafficOnly IsolateSOCKSAuth ExtendedErrors"])
            .args(["--SocksPolicy", "accept 127.0.0.1,reject *"])
            .args(["--ControlPort", "127.0.0.1:auto"])
            .args(["--ControlPortWriteToFile", &port_file.to_string_lossy()])
            .args(["--CookieAuthentication", "1"])
            .args(["--CookieAuthFile", &cookie_file.to_string_lossy()])
            .args(["--__OwningControllerProcess", &std::process::id().to_string()])
            .args(["--ClientOnly", "1"])
            .args(["--Log", "notice stderr"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            // nessuna finestra di console accanto all'app
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = cmd.spawn().map_err(err_avvio)?;

        // 2. aspetta il file con la porta di controllo
        let t0 = Instant::now();
        let control_port = loop {
            if let Ok(s) = std::fs::read_to_string(&port_file) {
                if let Some(p) = parse_port_file(&s) {
                    break p;
                }
            }
            if let Ok(Some(st)) = child.try_wait() {
                return Err(err_avvio(format!("tor è uscito subito ({st})")));
            }
            if t0.elapsed() > Duration::from_secs(30) {
                let _ = child.kill().await;
                return Err(err_avvio("tor non ha aperto la porta di controllo"));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        };

        // 3. autenticazione con cookie e presa di possesso
        let stream = TcpStream::connect(("127.0.0.1", control_port)).await.map_err(err_avvio)?;
        let mut control = BufReader::new(stream);
        let cookie = std::fs::read(&cookie_file).map_err(err_avvio)?;
        command(&mut control, &format!("AUTHENTICATE {}", hex(&cookie))).await?;
        command(&mut control, "TAKEOWNERSHIP").await?;
        // da qui tor si chiude quando questa connessione si chiude: il controllo
        // sul PID resta come seconda difesa se l'app viene uccisa
        command(&mut control, "RESETCONF __OwningControllerProcess").await?;

        // 4. porta SOCKS scelta da tor
        let lines = command(&mut control, "GETINFO net/listeners/socks").await?;
        let socks_port = lines
            .iter()
            .find_map(|l| l.strip_prefix("net/listeners/socks="))
            .and_then(|v| v.trim_matches('"').split_whitespace().next().map(str::to_string))
            .and_then(|a| a.trim_matches('"').strip_prefix("127.0.0.1:").and_then(|p| p.parse().ok()))
            .ok_or_else(|| err_avvio("porta SOCKS non trovata o non su 127.0.0.1"))?;

        Ok(Running { child, control, socks_port })
    }

    async fn bootstrap_progress(r: &mut Running) -> Result<u8> {
        let lines = command(&mut r.control, "GETINFO status/bootstrap-phase").await?;
        let p = lines
            .iter()
            .find_map(|l| l.split_whitespace().find_map(|w| w.strip_prefix("PROGRESS=")))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        Ok(p)
    }
}

/// SOCKS5 con nome/password (RFC 1929) verso un nome onion, porta `port`.
/// Causa leggibile di un codice di risposta SOCKS5 (RFC 1928) e dei codici estesi di tor per gli
/// onion (`ExtendedErrors`, tor-spec socks-extensions: 0xF0–0xF7).
fn causa_socks(codice: u8) -> &'static str {
    match codice {
        1 => "errore generico di tor",
        2 => "non consentito dalle regole (OnionTrafficOnly)",
        3 => "rete irraggiungibile",
        4 => "host irraggiungibile",
        5 => "connessione rifiutata dal servizio",
        6 => "tempo scaduto dentro Tor",
        7 => "comando non supportato",
        8 => "tipo di indirizzo non supportato",
        0xF0 => "descrittore dell'onion non trovato",
        0xF1 => "descrittore dell'onion non valido",
        0xF2 => "introduzione all'onion fallita",
        0xF3 => "rendezvous con l'onion fallito",
        0xF4 => "manca l'autorizzazione del client",
        0xF5 => "autorizzazione del client sbagliata",
        0xF6 => "indirizzo onion non valido",
        0xF7 => "introduzione all'onion scaduta",
        _ => "codice sconosciuto",
    }
}

/// Un tentativo di collegamento a un onion con tempo massimo TOTALE: oltre, la connessione SOCKS si
/// chiude e tor abbandona il flusso. Senza, un tentativo restava appeso ~120 s (SocksTimeout di tor).
async fn socks5_connect_con_limite(limite: Duration, socks_port: u16, user: &str, pass: &str, host: &str, port: u16) -> Result<TcpStream> {
    tokio::time::timeout(limite, socks5_connect(socks_port, user, pass, host, port))
        .await
        .map_err(|_| TransportError::Connessione(format!("tempo massimo del tentativo superato ({} s)", limite.as_secs())))?
}

async fn socks5_connect(socks_port: u16, user: &str, pass: &str, host: &str, port: u16) -> Result<TcpStream> {
    let e = |m: &str| TransportError::Connessione(m.to_string());
    let mut s = TcpStream::connect(("127.0.0.1", socks_port)).await.map_err(|x| e(&x.to_string()))?;
    s.write_all(&[5, 1, 2]).await.map_err(|x| e(&x.to_string()))?; // solo metodo 2: nome/password
    let mut b = [0u8; 2];
    s.read_exact(&mut b).await.map_err(|x| e(&x.to_string()))?;
    if b != [5, 2] {
        return Err(e("il SOCKS non accetta nome/password"));
    }
    let mut auth = vec![1, user.len() as u8];
    auth.extend_from_slice(user.as_bytes());
    auth.push(pass.len() as u8);
    auth.extend_from_slice(pass.as_bytes());
    s.write_all(&auth).await.map_err(|x| e(&x.to_string()))?;
    s.read_exact(&mut b).await.map_err(|x| e(&x.to_string()))?;
    if b[1] != 0 {
        return Err(e("credenziali SOCKS rifiutate"));
    }
    let mut req = vec![5, 1, 0, 3, host.len() as u8];
    req.extend_from_slice(host.as_bytes());
    req.extend_from_slice(&port.to_be_bytes());
    s.write_all(&req).await.map_err(|x| e(&x.to_string()))?;
    let mut head = [0u8; 4];
    s.read_exact(&mut head).await.map_err(|x| e(&x.to_string()))?;
    if head[1] != 0 {
        return Err(TransportError::Connessione(format!("SOCKS ha risposto {} ({})", head[1], causa_socks(head[1]))));
    }
    // salta l'indirizzo legato nella risposta
    let skip = match head[3] {
        1 => 4 + 2,
        4 => 16 + 2,
        3 => {
            let mut l = [0u8; 1];
            s.read_exact(&mut l).await.map_err(|x| e(&x.to_string()))?;
            l[0] as usize + 2
        }
        _ => return Err(e("risposta SOCKS malformata")),
    };
    let mut rest = vec![0u8; skip];
    s.read_exact(&mut rest).await.map_err(|x| e(&x.to_string()))?;
    Ok(s)
}

impl TorProcessTransport {
    /// Avvia tor e lo mette in sicurezza (hash, porte, cookie, possesso) SENZA aspettare
    /// la rete Tor. Le garanzie di I-37 valgono già da qui; `bootstrap` aspetta poi il 100%.
    pub async fn avvia(&self) -> Result<()> {
        let mut guard = self.running.lock().await;
        if guard.is_none() {
            self.set_status(TransportStatus::Avvio { percento: 0 });
            match self.start().await {
                Ok(r) => *guard = Some(r),
                Err(e) => {
                    self.set_status(TransportStatus::Errore(e.to_string()));
                    return Err(e);
                }
            }
        }
        Ok(())
    }
}

impl Transport for TorProcessTransport {
    async fn bootstrap(&self) -> Result<BootstrapInfo> {
        let t0 = Instant::now();
        self.avvia().await?;
        let mut guard = self.running.lock().await;
        let r = guard.as_mut().ok_or(TransportError::NonPronto)?;
        loop {
            let p = Self::bootstrap_progress(r).await?;
            self.set_status(if p >= 100 { TransportStatus::Pronto } else { TransportStatus::Avvio { percento: p } });
            if p >= 100 {
                break;
            }
            if t0.elapsed() > self.cfg.bootstrap_timeout {
                let e = TransportError::Avvio(format!("bootstrap fermo al {p}%"));
                self.set_status(TransportStatus::Errore(e.to_string()));
                return Err(e);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Ok(BootstrapInfo { millisecondi: t0.elapsed().as_millis(), implementazione: "tor-process" })
    }

    async fn connect_onion(&self, host: &OnionV3, port: u16, isolation: Isolation) -> Result<Stream> {
        if port == 0 {
            return Err(TransportError::Porta);
        }
        let socks = self.socks_port().await.ok_or(TransportError::NonPronto)?;
        // gettoni diversi = credenziali diverse = circuiti diversi (IsolateSOCKSAuth)
        let user = format!("m4tr1x-{:016x}", isolation.0);
        let s = socks5_connect_con_limite(crate::TEMPO_MASSIMO_CONNESSIONE, socks, &user, &self.session_secret, &host.host(), port).await?;
        Ok(Box::pin(s))
    }

    fn status(&self) -> TransportStatus {
        self.status.lock().unwrap().clone()
    }

    async fn shutdown(&self) {
        if let Some(mut r) = self.running.lock().await.take() {
            let _ = command(&mut r.control, "SIGNAL SHUTDOWN").await;
            if tokio::time::timeout(Duration::from_secs(10), r.child.wait()).await.is_err() {
                let _ = r.child.kill().await;
            }
        }
        self.set_status(TransportStatus::Spento);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_della_porta_solo_loopback() {
        assert_eq!(parse_port_file("PORT=127.0.0.1:9051\n"), Some(9051));
        assert_eq!(parse_port_file("PORT=0.0.0.0:9051\n"), None);
        assert_eq!(parse_port_file("PORT=192.0.2.7:9051\n"), None);
        assert_eq!(parse_port_file(""), None);
    }

    #[test]
    fn segreti_di_sessione_diversi() {
        assert_ne!(random_hex(16), random_hex(16));
        assert_eq!(random_hex(16).len(), 32);
    }

    /// Finto SOCKS che accetta nome/password e poi risponde alla richiesta con `codice`, oppure (None) tace.
    async fn socks_finto(codice: Option<u8>) -> u16 {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let mut b = [0u8; 512];
            let _ = s.read(&mut b).await; // saluto
            s.write_all(&[5, 2]).await.unwrap();
            let _ = s.read(&mut b).await; // credenziali
            s.write_all(&[1, 0]).await.unwrap();
            let _ = s.read(&mut b).await; // richiesta
            match codice {
                Some(c) => s.write_all(&[5, c, 0, 1, 0, 0, 0, 0, 0, 0]).await.unwrap(),
                None => tokio::time::sleep(Duration::from_secs(3600)).await,
            }
        });
        p
    }

    #[tokio::test]
    async fn codici_estesi_di_tor_leggibili() {
        let p = socks_finto(Some(0xF0)).await;
        let e = socks5_connect_con_limite(Duration::from_secs(5), p, "u", "p", "x.onion", 80).await.unwrap_err();
        assert!(e.to_string().contains("240 (descrittore dell'onion non trovato)"), "{e}");
        assert_eq!(causa_socks(6), "tempo scaduto dentro Tor");
        assert_eq!(causa_socks(0xF2), "introduzione all'onion fallita");
    }

    #[tokio::test(start_paused = true)]
    async fn tentativo_appeso_chiuso_al_tempo_massimo() {
        let p = socks_finto(None).await;
        let t0 = tokio::time::Instant::now();
        let e = socks5_connect_con_limite(crate::TEMPO_MASSIMO_CONNESSIONE, p, "u", "p", "x.onion", 80).await.unwrap_err();
        assert!(e.to_string().contains("tempo massimo del tentativo"), "{e}");
        assert!(t0.elapsed() >= crate::TEMPO_MASSIMO_CONNESSIONE && t0.elapsed() < crate::TEMPO_MASSIMO_CONNESSIONE * 2);
    }
}
