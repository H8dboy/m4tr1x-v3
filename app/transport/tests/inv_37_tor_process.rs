//! I-37 — Il Tor di Windows serve solo onion e non presta i circuiti dell'app.
//! I-36 — `connect_onion` raggiunge davvero un servizio onion (prova positiva).
//!
//! Le prove con tor vero girano solo se `M4TR1X_TOR_BIN` indica un binario tor
//! (in CI: job "tor"). Senza, vengono saltate e lo dicono.

use m4tr1x_transport::tor_process::sha256_file;
use m4tr1x_transport::{Isolation, OnionV3, TorProcessConfig, TorProcessTransport, Transport, TransportStatus};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

fn tor_bin() -> Option<PathBuf> {
    match std::env::var("M4TR1X_TOR_BIN") {
        Ok(p) if !p.is_empty() => Some(p.into()),
        _ => {
            eprintln!("SALTATO: serve M4TR1X_TOR_BIN");
            None
        }
    }
}

fn tmpdir(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("m4tr1x-i37-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Cache del consenso di Tor tra i test (stessa cartella e stesse regole di
/// tests/helpers/node.js): solo i file `cached-*`, firmati dalle autorità e verificati
/// da tor al caricamento; mai chiavi né `state`. M4TR1X_TOR_CACHE=0 la spegne.
const CACHE_FILES: [&str; 5] = ["cached-certs", "cached-microdesc-consensus", "cached-microdescs", "cached-microdescs.new", "cached-consensus"];

fn cache_dir() -> Option<PathBuf> {
    (std::env::var("M4TR1X_TOR_CACHE").as_deref() != Ok("0")).then(|| std::env::temp_dir().join("m4tr1x-tor-cache"))
}

/// Copia la cache nella DataDirectory di tor. true se c'era qualcosa.
fn cache_semina(data: &Path) -> bool {
    let Some(c) = cache_dir() else { return false };
    std::fs::create_dir_all(data).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(data, std::fs::Permissions::from_mode(0o700));
    }
    CACHE_FILES.iter().filter(|f| std::fs::copy(c.join(f), data.join(f)).is_ok()).count() > 0
}

/// Salva i file freschi nella cache (copia + rinomina: scrittura atomica).
fn cache_salva(data: &Path) {
    let Some(c) = cache_dir() else { return };
    let _ = std::fs::create_dir_all(&c);
    for f in CACHE_FILES {
        let tmp = c.join(format!("{f}.{}.tmp", std::process::id()));
        if std::fs::copy(data.join(f), &tmp).is_ok() && std::fs::rename(&tmp, c.join(f)).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

/// DataDirectory usata da TorProcessTransport: <data_dir>/tor/data.
fn data_tor(dir: &Path) -> PathBuf {
    dir.join("tor").join("data")
}

fn transport(bin: &Path, dir: &Path) -> TorProcessTransport {
    let calda = cache_semina(&data_tor(dir));
    eprintln!("cache di tor: {}", if calda { "calda" } else { "fredda" });
    TorProcessTransport::new(TorProcessConfig {
        tor_bin: bin.to_path_buf(),
        tor_sha256: sha256_file(bin).unwrap(),
        data_dir: dir.to_path_buf(),
        bootstrap_timeout: Duration::from_secs(180),
    })
}

fn pid_vivo(pid: u32) -> bool {
    if cfg!(windows) {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
    } else {
        std::process::Command::new("kill").args(["-0", &pid.to_string()]).status().map(|s| s.success()).unwrap_or(false)
    }
}

fn uccidi(pid: u32) {
    let _ = if cfg!(windows) {
        std::process::Command::new("taskkill").args(["/F", "/PID", &pid.to_string()]).output()
    } else {
        std::process::Command::new("kill").args(["-9", &pid.to_string()]).output()
    };
}

async fn aspetta_morte(pid: u32, max: Duration) -> bool {
    let t0 = Instant::now();
    while t0.elapsed() < max {
        if !pid_vivo(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}

/// SOCKS5 SENZA credenziali (metodo 0) verso un indirizzo in chiaro: codice di risposta.
async fn socks_senza_credenziali(port: u16, atyp: u8, addr: &[u8]) -> u8 {
    let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    s.write_all(&[5, 1, 0]).await.unwrap();
    let mut b = [0u8; 2];
    s.read_exact(&mut b).await.unwrap();
    assert_eq!(b, [5, 0], "tor accetta il metodo senza credenziali (atteso: I-37 non lo promette)");
    let mut req = vec![5, 1, 0, atyp];
    req.extend_from_slice(addr);
    req.extend_from_slice(&80u16.to_be_bytes());
    s.write_all(&req).await.unwrap();
    let mut head = [0u8; 4];
    s.read_exact(&mut head).await.unwrap();
    head[1]
}

/// Un IP locale non di loopback, se la macchina ne ha uno (UDP "connect" non manda pacchetti).
fn ip_locale_non_loopback() -> Option<std::net::IpAddr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?; // RFC 5737, solo per scegliere l'interfaccia
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

#[tokio::test]
async fn binario_con_hash_sbagliato_non_parte() {
    let dir = tmpdir("hash");
    let finto = dir.join("tor-finto.exe");
    std::fs::write(&finto, b"non sono tor").unwrap();
    let t = TorProcessTransport::new(TorProcessConfig {
        tor_bin: finto,
        tor_sha256: "00".repeat(32),
        data_dir: dir.clone(),
        bootstrap_timeout: Duration::from_secs(5),
    });
    let e = t.bootstrap().await.unwrap_err().to_string();
    assert!(e.contains("hash"), "errore inatteso: {e}");
    assert!(t.tor_pid().await.is_none(), "nessun processo deve partire");
}

#[tokio::test(flavor = "multi_thread")]
async fn tor_vero_solo_onion_solo_loopback_cookie_e_chiusura() {
    let Some(bin) = tor_bin() else { return };
    let dir = tmpdir("vero");
    let t = transport(&bin, &dir);
    // Le garanzie di I-37 valgono appena tor è partito: non serve aspettare la rete
    // Tor (in CI il bootstrap a volte si ferma a metà per minuti e falsava la prova).
    t.avvia().await.expect("avvio di tor");
    let socks = t.socks_port().await.unwrap();

    // 1. senza credenziali: host in chiaro rifiutati PER REGOLA (dominio e IPv4).
    //    Non basta "non raggiunto": un host può fallire per mille motivi. Serve il
    //    codice SOCKS 2 ("non consentito dalle regole"), quello di OnionTrafficOnly.
    let dom = b"example.com";
    let mut a = vec![dom.len() as u8];
    a.extend_from_slice(dom);
    assert_eq!(socks_senza_credenziali(socks, 3, &a).await, 2, "dominio in chiaro non rifiutato per regola");
    assert_eq!(socks_senza_credenziali(socks, 1, &[198, 51, 100, 7]).await, 2, "IPv4 in chiaro non rifiutato per regola");

    // 2. porta di controllo: senza cookie niente
    let port_file = std::fs::read_to_string(t.control_port_file()).unwrap();
    let ctl: u16 = port_file.trim().rsplit_once(':').unwrap().1.parse().unwrap();
    let mut c = BufReader::new(TcpStream::connect(("127.0.0.1", ctl)).await.unwrap());
    c.get_mut().write_all(b"AUTHENTICATE 00\r\n").await.unwrap();
    let mut line = String::new();
    c.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("515"), "autenticazione sbagliata accettata: {line}");

    // 3. niente in ascolto fuori da 127.0.0.1
    match ip_locale_non_loopback() {
        Some(ip) => {
            for p in [socks, ctl] {
                let r = tokio::time::timeout(Duration::from_secs(3), TcpStream::connect((ip, p))).await;
                assert!(!matches!(r, Ok(Ok(_))), "porta {p} raggiungibile da {ip}");
            }
        }
        None => eprintln!("controllo interfacce SALTATO: nessun IP non di loopback"),
    }

    // 4. shutdown: il processo tor sparisce
    let pid = t.tor_pid().await.unwrap();
    t.shutdown().await;
    cache_salva(&data_tor(&dir));
    assert!(aspetta_morte(pid, Duration::from_secs(15)).await, "tor ancora vivo dopo shutdown");
}

#[tokio::test(flavor = "multi_thread")]
async fn uccidendo_l_app_tor_sparisce() {
    let Some(bin) = tor_bin() else { return };
    let dir = tmpdir("owner");
    // La parte dell'app la fa QUESTO STESSO eseguibile, rilanciato sulla prova
    // `aiuto_fa_l_app`: così è sempre compilato dal sorgente attuale. (Un esempio
    // di cargo no: `cargo test --test …` non lo ricompila, e una volta ha girato
    // una copia vecchia e sabotata, falsando la prova di sabotaggio.)
    let mut app = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args(["aiuto_fa_l_app", "--exact", "--nocapture", "--test-threads=1"])
        .env("M4TR1X_TOR_BIN", &bin)
        .env("M4TR1X_I37_APP_DIR", &dir)
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut out = BufReader::new(app.stdout.take().unwrap());
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(60), out.read_line(&mut line)).await.expect("PID non stampato").unwrap();
    let pid: u32 = loop {
        // il test harness stampa anche le sue righe, e "test aiuto_fa_l_app ... " senza
        // andare a capo: il PID può stare a metà riga
        if let Some((_, p)) = line.trim().split_once("PID ") {
            break p.trim().parse().unwrap();
        }
        line.clear();
        let n = tokio::time::timeout(Duration::from_secs(60), out.read_line(&mut line)).await.expect("PID non stampato").unwrap();
        assert!(n > 0, "l'app di prova è uscita senza stampare il PID");
    };
    assert!(pid_vivo(pid));
    app.kill().await.unwrap(); // morte improvvisa dell'app, senza shutdown
    let morto = aspetta_morte(pid, Duration::from_secs(30)).await;
    if !morto {
        // Pulizia prima di fallire: su Windows l'orfano eredita le maniglie dell'uscita
        // del test e terrebbe appeso chi la legge (cargo, il runner dei sabotaggi).
        uccidi(pid);
    }
    assert!(morto, "tor sopravvive all'app");
}

/// Non è una prova: è "l'app" per `uccidendo_l_app_tor_sparisce`, che rilancia questo
/// eseguibile con M4TR1X_I37_APP_DIR. Avvia il trasporto, stampa il PID di tor e
/// resta in attesa finché non viene ucciso. Senza la variabile non fa niente.
#[tokio::test(flavor = "multi_thread")]
async fn aiuto_fa_l_app() {
    let Ok(dir) = std::env::var("M4TR1X_I37_APP_DIR") else { return };
    let bin = tor_bin().expect("M4TR1X_TOR_BIN");
    let t = TorProcessTransport::new(TorProcessConfig {
        tor_sha256: sha256_file(&bin).unwrap(),
        tor_bin: bin,
        data_dir: dir.into(),
        bootstrap_timeout: Duration::from_secs(1),
    });
    t.avvia().await.expect("avvio di tor"); // basta che tor sia partito, non serve la rete
    println!("PID {}", t.tor_pid().await.expect("tor avviato"));
    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

/// Un evento `HS_DESC` di un tor: millisecondi dall'inizio della prova, azione, HSDir (8 cifre), causa.
#[derive(Clone, Debug)]
struct EventoHs {
    ms: u128,
    azione: String,
    hsdir: String,
    causa: String,
}

type Registro = std::sync::Arc<std::sync::Mutex<Vec<EventoHs>>>;

/// Ascolta gli eventi `HS_DESC` di un tor (porta di controllo + cookie) per l'onion `indirizzo`, finché la
/// prova dura. Serve a confrontare gli HSDir su cui il servizio CARICA con quelli a cui il client CHIEDE
/// (misura del 20/10/2026: primo collegamento a volte 1–3 minuti, con "descrittore non trovato").
async fn ascolta_hs_desc(file_porta: PathBuf, file_cookie: PathBuf, indirizzo: String, t0: Instant) -> Registro {
    let attesa = Instant::now();
    let porta = loop {
        if let Some(p) = std::fs::read_to_string(&file_porta).ok().and_then(|s| {
            let v = s.lines().find_map(|l| l.trim().strip_prefix("PORT=").map(str::to_owned))?;
            let (host, port) = v.rsplit_once(':')?;
            (host == "127.0.0.1").then(|| port.parse::<u16>().ok()).flatten()
        }) {
            break p;
        }
        assert!(attesa.elapsed() < Duration::from_secs(60), "porta di controllo non aperta: {}", file_porta.display());
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    let cookie: String = std::fs::read(&file_cookie).unwrap().iter().map(|b| format!("{b:02x}")).collect();
    let mut c = BufReader::new(TcpStream::connect(("127.0.0.1", porta)).await.unwrap());
    c.get_mut().write_all(format!("AUTHENTICATE {cookie}\r\nSETEVENTS HS_DESC\r\n").as_bytes()).await.unwrap();
    let registro: Registro = Default::default();
    let r = registro.clone();
    tokio::spawn(async move {
        let mut riga = String::new();
        loop {
            riga.clear();
            if !matches!(c.read_line(&mut riga).await, Ok(n) if n > 0) {
                return;
            }
            // 650 HS_DESC <azione> <indirizzo> <auth> <HSDir> [<id>] [REASON=…]
            let p: Vec<&str> = riga.split_whitespace().collect();
            if p.len() >= 6 && p[0] == "650" && p[1] == "HS_DESC" && p[3] == indirizzo {
                let hsdir: String = p[5].trim_start_matches('$').chars().take(8).collect();
                let causa = p.iter().find_map(|x| x.strip_prefix("REASON=")).unwrap_or("").to_owned();
                r.lock().unwrap().push(EventoHs { ms: t0.elapsed().as_millis(), azione: p[2].to_owned(), hsdir, causa });
            }
        }
    });
    registro
}

/// Aspetta che il tor del servizio di prova abbia pubblicato il descrittore: almeno 12 `HS_DESC UPLOADED`
/// su 16 (8 HSDir del periodo attuale + 8 del prossimo; tor a volte carica prima il prossimo, quindi
/// 12 garantisce anche l'attuale). Riserva a 300 s come il nodo (I-59): poi si prova lo stesso.
/// Restituisce (UPLOADED, secondi, riserva usata).
async fn attendi_pubblicazione(servizio: Registro, t0: Instant) -> (usize, u64, bool) {
    const SERVONO: usize = 12;
    loop {
        let caricati = servizio.lock().unwrap().iter().filter(|e| e.azione == "UPLOADED").count();
        if caricati >= SERVONO {
            return (caricati, t0.elapsed().as_secs(), false);
        }
        if t0.elapsed() > Duration::from_secs(300) {
            return (caricati, t0.elapsed().as_secs(), true);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// "+12.3s REQUESTED 1A2B3C4D [caricato qui: sì]" per ogni evento del client. "tentato" = il servizio ci ha
/// provato (UPLOAD) ma la conferma non è arrivata; "NO" = mai tentato, cioè fuori dall'anello del servizio.
fn racconta_client(client: &[EventoHs], caricati: &std::collections::HashSet<String>, tentati: &std::collections::HashSet<String>) -> String {
    client
        .iter()
        .map(|e| {
            let qui = if caricati.contains(&e.hsdir) {
                "sì"
            } else if tentati.contains(&e.hsdir) {
                "tentato"
            } else {
                "NO"
            };
            let causa = if e.causa.is_empty() { String::new() } else { format!(" {}", e.causa) };
            format!("+{:.1}s {} {} [caricato qui: {qui}]{causa}", e.ms as f64 / 1000.0, e.azione, e.hsdir)
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Prova positiva: un servizio onion di prova (tor separato, nel test) con un'eco;
/// l'app lo raggiunge con connect_onion e riceve indietro quello che manda.
#[tokio::test(flavor = "multi_thread")]
async fn connect_onion_raggiunge_un_servizio_onion() {
    let Some(bin) = tor_bin() else { return };
    let dir = tmpdir("onion");

    let eco = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let eco_port = eco.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = eco.accept().await {
            tokio::spawn(async move {
                let mut b = [0u8; 64];
                if let Ok(n) = s.read(&mut b).await {
                    let _ = s.write_all(&b[..n]).await;
                }
            });
        }
    });

    // tor del servizio: processo a parte, solo per il test (l'app non ospita servizi)
    let hs = dir.join("hs");
    std::fs::create_dir_all(&hs).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hs, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    cache_semina(&dir.join("hs-data"));
    let mut servizio = tokio::process::Command::new(&bin)
        .args(["-f", &dir.join("nessun-torrc").to_string_lossy()])
        .arg("--ignore-missing-torrc")
        .args(["--DataDirectory", &dir.join("hs-data").to_string_lossy()])
        .args(["--SocksPort", "0"])
        .args(["--HiddenServiceDir", &hs.to_string_lossy()])
        .args(["--HiddenServicePort", &format!("80 127.0.0.1:{eco_port}")])
        // porta di controllo SOLO per sapere quando il descrittore è pubblicato (punto 42, 20/10/2026)
        .args(["--ControlPort", "auto", "--ControlPortWriteToFile", &dir.join("hs-control.port").to_string_lossy()])
        .args(["--CookieAuthentication", "1", "--CookieAuthFile", &dir.join("hs-control.cookie").to_string_lossy()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let hostname = hs.join("hostname");
    let t0 = Instant::now();
    let host = loop {
        if let Ok(h) = std::fs::read_to_string(&hostname) {
            break h.trim().to_string();
        }
        assert!(t0.elapsed() < Duration::from_secs(60), "il servizio di prova non ha creato l'onion");
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    let onion = OnionV3::parse(&host).expect("tor ha scritto un onion v3 valido");
    // la pubblicazione corre in parallelo al bootstrap dell'app; si registrano gli HSDir del servizio
    let indirizzo = host.trim_end_matches(".onion").to_string();
    let t_prova = Instant::now();
    let reg_servizio = ascolta_hs_desc(dir.join("hs-control.port"), dir.join("hs-control.cookie"), indirizzo.clone(), t_prova).await;
    let pubblicazione = tokio::spawn(attendi_pubblicazione(reg_servizio.clone(), t_prova));

    let t = transport(&bin, &dir.join("app"));
    // Questa è la prova che il trasporto funziona davvero: qui si aspetta il bootstrap
    // COMPLETO della rete Tor (mai `avvia()`), e si controlla che sia arrivato al 100%.
    t.bootstrap().await.expect("bootstrap");
    assert_eq!(t.status(), TransportStatus::Pronto, "bootstrap() è tornato prima del 100%");
    // Fase "pubblicazione" (rete Tor): si aspetta il descrittore, non si riprova alla cieca.
    // Fase "connessione" (il nostro trasporto): da qui un rosso NON è più la rete che pubblica.
    let (caricati, secondi, riserva) = pubblicazione.await.unwrap();
    // da qui si registra anche cosa CHIEDE il client (stessa porta di controllo dell'app, con il suo cookie)
    let client = ascolta_hs_desc(t.control_port_file(), dir.join("app").join("tor").join("control.cookie"), indirizzo.clone(), t_prova).await;
    eprintln!("pubblicazione dell'onion di prova: {caricati}/16 UPLOADED in {secondi} s{}", if riserva { " (riserva: tempo scaduto, si prova lo stesso)" } else { "" });
    let t1 = Instant::now();
    // registro dei tentativi: quando parte, quanto dura, come finisce (20/10: 124 s dopo una pubblicazione di 7 s)
    let mut tentativi: Vec<String> = vec![];
    let ok = loop {
        let ta = Instant::now();
        let partenza = t1.elapsed().as_secs();
        match t.connect_onion(&onion, 80, Isolation(1)).await {
            Ok(mut s) => {
                tentativi.push(format!("t+{partenza}s ok in {} ms", ta.elapsed().as_millis()));
                s.write_all(b"ciao nderja").await.unwrap();
                let mut b = [0u8; 11];
                s.read_exact(&mut b).await.unwrap();
                break &b == b"ciao nderja";
            }
            Err(e) => {
                tentativi.push(format!("t+{partenza}s {e} dopo {} ms", ta.elapsed().as_millis()));
                // stessa lingua di tutte le prove con Tor (runner: un rosso di rete si rilancia una volta):
                // pubblicazione non confermata → "pubblicazione"; confermata ma onion non raggiunto → "rete"
                let fase = if riserva { "pubblicazione" } else { "rete" };
                assert!(t1.elapsed() < Duration::from_secs(240), "fase \"{fase}\": onion non raggiunto: {e} · tentativi: {}", tentativi.join(" | "));
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    };
    assert!(ok, "fase \"eco\": l'eco non torna uguale"); // il nostro trasporto: mai rilanciato
    eprintln!("onion raggiunto in {} s", t1.elapsed().as_secs());
    eprintln!("tentativi di connessione: {}", tentativi.join(" | "));
    let caricati_su: std::collections::HashSet<String> =
        reg_servizio.lock().unwrap().iter().filter(|e| e.azione == "UPLOADED").map(|e| e.hsdir.clone()).collect();
    eprintln!("hsdir del servizio (UPLOADED): {}", caricati_su.iter().cloned().collect::<Vec<_>>().join(" "));
    let tentati_su: std::collections::HashSet<String> =
        reg_servizio.lock().unwrap().iter().filter(|e| e.azione == "UPLOAD").map(|e| e.hsdir.clone()).collect();
    eprintln!("hsdir del client: {}", racconta_client(&client.lock().unwrap(), &caricati_su, &tentati_su));
    t.shutdown().await;
    cache_salva(&data_tor(&dir.join("app")));
    let _ = servizio.kill().await;
}
