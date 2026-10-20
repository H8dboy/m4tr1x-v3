//! `TorProcessTransport` — Windows (ADR-0016, I-37).
//!
//! L'app avvia il `tor` classico incluso nel pacchetto e ci parla via SOCKS5 e
//! porta di controllo. Cosa garantisce (I-37, corretta il 16/10/2026):
//!   - binario con hash SHA-256 fissato, controllato PRIMA di avviarlo;
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

/// Configurazione: da dove viene tor, che hash deve avere, dove tiene i dati.
#[derive(Clone, Debug)]
pub struct TorProcessConfig {
    pub tor_bin: PathBuf,
    /// SHA-256 atteso del binario, esadecimale minuscolo (da `app/tor-windows.lock.json`).
    pub tor_sha256: String,
    /// Cartella dati dell'app: tor usa `<data_dir>/tor`, mai condivisa con altri Tor.
    pub data_dir: PathBuf,
    /// Tempo massimo per arrivare al 100% del bootstrap.
    pub bootstrap_timeout: Duration,
}

/// Lock del tor per Windows (`app/tor-windows.lock.json`), incluso nel programma alla COMPILAZIONE:
/// l'hash atteso non si legge da disco mentre l'app gira, quindi non si può cambiare accanto all'app.
const LOCK_TOR_WINDOWS: &str = include_str!("../../tor-windows.lock.json");

/// SHA-256 fissato del tor.exe che l'app accetta (campo `tor_exe_sha256` del lock).
pub fn sha256_tor_windows() -> String {
    let v: serde_json::Value = serde_json::from_str(LOCK_TOR_WINDOWS).expect("app/tor-windows.lock.json non valido");
    let h = v["tor_exe_sha256"].as_str().expect("tor_exe_sha256 mancante nel lock").to_owned();
    assert!(h.len() == 64 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "tor_exe_sha256 non è un SHA-256");
    h
}

impl TorProcessConfig {
    /// Configurazione dell'APP: tor da `tor_bin` (in F4 la variabile `M4TR1X_TOR_BIN`; dall'installer in F8),
    /// hash SOLO dal lock, cartella dati dell'app, 180 s per il bootstrap (come il nodo).
    /// `None` se tor non è indicato: il trasporto resta spento e l'app lo mostra.
    pub fn per_l_app(tor_bin: Option<PathBuf>, data_dir: PathBuf) -> Option<Self> {
        Some(Self {
            tor_bin: tor_bin.filter(|p| !p.as_os_str().is_empty())?,
            tor_sha256: sha256_tor_windows(),
            data_dir,
            bootstrap_timeout: Duration::from_secs(180),
        })
    }
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
        // 1. hash del binario PRIMA di eseguirlo
        let actual = sha256_file(&self.cfg.tor_bin).map_err(err_avvio)?;
        if actual != self.cfg.tor_sha256.to_ascii_lowercase() {
            return Err(err_avvio("hash del binario tor diverso da quello fissato"));
        }
        let dir = self.cfg.data_dir.join("tor");
        std::fs::create_dir_all(&dir).map_err(err_avvio)?;
        let port_file = dir.join("control.port");
        let cookie_file = dir.join("control.cookie");
        let _ = std::fs::remove_file(&port_file);

        let mut cmd = Command::new(&self.cfg.tor_bin);
        cmd.arg("--ignore-missing-torrc")
            // nessun torrc di sistema: solo questi argomenti (i due file indicati non esistono)
            .args(["-f", &dir.join("nessun-torrc").to_string_lossy()])
            .args(["--defaults-torrc", &dir.join("nessun-torrc-defaults").to_string_lossy()])
            .args(["--DataDirectory", &dir.join("data").to_string_lossy()])
            .args(["--SocksPort", "127.0.0.1:auto OnionTrafficOnly IsolateSOCKSAuth"])
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
        return Err(TransportError::Connessione(format!("SOCKS ha risposto {}", head[1])));
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
        let s = socks5_connect(socks, &user, &self.session_secret, &host.host(), port).await?;
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
}
