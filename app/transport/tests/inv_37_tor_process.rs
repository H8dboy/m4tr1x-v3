//! I-37 — Il Tor di Windows serve solo onion e non presta i circuiti dell'app.
//! I-36 — `connect_onion` raggiunge davvero un servizio onion (prova positiva).
//!
//! Le prove con tor vero girano solo se `M4TR1X_TOR_BIN` indica un binario tor
//! (in CI: job "tor"). Senza, vengono saltate e lo dicono.

use m4tr1x_transport::tor_process::sha256_file;
use m4tr1x_transport::{Isolation, OnionV3, TorProcessConfig, TorProcessTransport, Transport};
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

fn transport(bin: &Path, dir: &Path) -> TorProcessTransport {
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
    t.bootstrap().await.expect("bootstrap");
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
    let _ = t.bootstrap().await; // basta che tor sia partito, non serve il 100%
    println!("PID {}", t.tor_pid().await.expect("tor avviato"));
    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
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
    let mut servizio = tokio::process::Command::new(&bin)
        .args(["-f", &dir.join("nessun-torrc").to_string_lossy()])
        .arg("--ignore-missing-torrc")
        .args(["--DataDirectory", &dir.join("hs-data").to_string_lossy()])
        .args(["--SocksPort", "0"])
        .args(["--HiddenServiceDir", &hs.to_string_lossy()])
        .args(["--HiddenServicePort", &format!("80 127.0.0.1:{eco_port}")])
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

    let t = transport(&bin, &dir.join("app"));
    t.bootstrap().await.expect("bootstrap");
    // il descrittore del servizio impiega un po' a essere pubblicato: si riprova
    let t1 = Instant::now();
    let ok = loop {
        match t.connect_onion(&onion, 80, Isolation(1)).await {
            Ok(mut s) => {
                s.write_all(b"ciao nderja").await.unwrap();
                let mut b = [0u8; 11];
                s.read_exact(&mut b).await.unwrap();
                break &b == b"ciao nderja";
            }
            Err(e) => assert!(t1.elapsed() < Duration::from_secs(240), "onion non raggiunto: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    };
    assert!(ok, "l'eco non torna uguale");
    eprintln!("onion raggiunto in {} s", t1.elapsed().as_secs());
    t.shutdown().await;
    let _ = servizio.kill().await;
}
