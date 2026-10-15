//! Prototipo F4a — l'app si collega a un servizio onion SOLO tramite Arti.
//!
//! Comandi esposti all'interfaccia:
//!   bootstrap()      avvia Arti e misura il tempo fino al circuito pronto
//!   fetch(url, ...)  GET http://<onion>/<path> via Arti: tempo al primo byte,
//!                    durata, byte, velocità media, SHA-256 confrontato con x-sha256
//!
//! Nessun'altra libreria di rete è usata: l'unica via d'uscita è Arti.

use std::sync::Arc;
use std::time::Instant;

use arti_client::config::CfgPath;
use arti_client::{TorClient, TorClientConfig};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::Manager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::OnceCell;
use tor_rtcompat::PreferredRuntime;

static TOR: OnceCell<Arc<TorClient<PreferredRuntime>>> = OnceCell::const_new();

#[derive(Serialize)]
struct BootstrapResult {
    ms: u128,
    already: bool,
}

#[derive(Serialize, Debug)]
pub struct FetchResult {
    status: String,
    connect_ms: u128,
    first_byte_ms: u128,
    total_ms: u128,
    body_bytes: usize,
    mb_per_s: f64,
    sha256: String,
    sha256_expected: Option<String>,
    sha256_ok: Option<bool>,
}

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Crea e avvia un client Arti con stato e cache in `base`.
pub async fn make_client(base: std::path::PathBuf) -> Result<Arc<TorClient<PreferredRuntime>>, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut b = TorClientConfig::builder();
    b.storage()
        .state_dir(CfgPath::new_literal(base.join("arti-state")))
        .cache_dir(CfgPath::new_literal(base.join("arti-cache")));
    b.address_filter().allow_onion_addrs(true);
    let cfg = b.build().map_err(err)?;
    TorClient::create_bootstrapped(cfg).await.map_err(err)
}

async fn tor(app: &tauri::AppHandle) -> Result<(&'static Arc<TorClient<PreferredRuntime>>, bool), String> {
    if let Some(c) = TOR.get() {
        return Ok((c, true));
    }
    let base = app.path().app_data_dir().map_err(err)?;
    let client = make_client(base).await?;
    let _ = TOR.set(client);
    Ok((TOR.get().expect("appena impostato"), false))
}

#[tauri::command]
async fn bootstrap(app: tauri::AppHandle) -> Result<BootstrapResult, String> {
    let t0 = Instant::now();
    let (_, already) = tor(&app).await?;
    Ok(BootstrapResult { ms: t0.elapsed().as_millis(), already })
}

/// Solo http://<56 caratteri>.onion/<path>: nessun host in chiaro è ammesso.
fn parse_onion_url(url: &str) -> Result<(String, String), String> {
    let rest = url.strip_prefix("http://").ok_or("solo http://<onion>")?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let label = host.strip_suffix(".onion").ok_or("solo indirizzi .onion")?;
    if label.len() != 56 || !label.chars().all(|c| c.is_ascii_lowercase() || ('2'..='7').contains(&c)) {
        return Err("indirizzo onion v3 non valido".into());
    }
    Ok((host.to_string(), path.to_string()))
}

#[tauri::command]
async fn fetch(app: tauri::AppHandle, url: String) -> Result<FetchResult, String> {
    let (client, _) = tor(&app).await?;
    // circuito nuovo a ogni richiesta: ogni misura è indipendente
    fetch_via(&client.isolated_client(), &url).await
}

/// GET via Arti. Usato sia dall'interfaccia sia dalla modalità --bench.
pub async fn fetch_via(client: &TorClient<PreferredRuntime>, url: &str) -> Result<FetchResult, String> {
    let (host, path) = parse_onion_url(url)?;
    let t0 = Instant::now();
    let mut stream = client.connect((host.as_str(), 80)).await.map_err(err)?;
    let connect_ms = t0.elapsed().as_millis();
    let req = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nUser-Agent: m4tr1x-spike\r\n\r\n");
    stream.write_all(req.as_bytes()).await.map_err(err)?;
    stream.flush().await.map_err(err)?;

    let mut buf = Vec::with_capacity(11 * 1024 * 1024);
    let mut chunk = vec![0u8; 64 * 1024];
    let mut first_byte_ms = 0;
    loop {
        let n = stream.read(&mut chunk).await.map_err(err)?;
        if n == 0 {
            break;
        }
        if buf.is_empty() {
            first_byte_ms = t0.elapsed().as_millis();
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > 64 * 1024 * 1024 {
            return Err("risposta oltre 64 MB: interrotta".into());
        }
    }
    let total_ms = t0.elapsed().as_millis();

    let split = buf.windows(4).position(|w| w == b"\r\n\r\n").ok_or("risposta HTTP senza intestazioni")?;
    let head = String::from_utf8_lossy(&buf[..split]).to_string();
    let body = &buf[split + 4..];
    let status = head.lines().next().unwrap_or("").to_string();
    let expected = head
        .lines()
        .find_map(|l| l.to_ascii_lowercase().strip_prefix("x-sha256:").map(|v| v.trim().to_string()));
    let sha = format!("{:x}", Sha256::digest(body));
    let secs = (total_ms.saturating_sub(first_byte_ms).max(1)) as f64 / 1000.0;
    Ok(FetchResult {
        status,
        connect_ms,
        first_byte_ms,
        total_ms,
        body_bytes: body.len(),
        mb_per_s: (body.len() as f64 / 1_048_576.0) / secs,
        sha256_ok: expected.as_ref().map(|e| e == &sha),
        sha256_expected: expected,
        sha256: sha,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![bootstrap, fetch])
        .run(tauri::generate_context!())
        .expect("errore all'avvio dell'app");
}

/// Modalità a riga di comando: `m4tr1x-spike-f4a --bench http://<onion> <cartella-dati-nuova>`
/// Misura avvio a freddo, 10 collegamenti su circuiti nuovi, download di 10 MB.
pub fn bench(base_url: String, dir: std::path::PathBuf) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr).try_init();
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
    rt.block_on(async move {
        let t0 = Instant::now();
        let client = match make_client(dir).await {
            Ok(c) => c,
            Err(e) => { println!("{{\"bootstrap_error\":{:?}}}", e); return; }
        };
        println!("{{\"bootstrap_ms\":{}}}", t0.elapsed().as_millis());
        let mut within_60 = 0;
        for i in 1..=10 {
            let t = Instant::now();
            let r = tokio::time::timeout(std::time::Duration::from_secs(90),
                fetch_via(&client.isolated_client(), &format!("{base_url}/health"))).await;
            let ms = t.elapsed().as_millis();
            let ok = matches!(r, Ok(Ok(_)));
            if ok && ms <= 60_000 { within_60 += 1; }
            println!("{{\"connect\":{i},\"ok\":{ok},\"ms\":{ms}}}");
        }
        println!("{{\"connect_within_60s\":{within_60},\"of\":10}}");
        match fetch_via(&client.isolated_client(), &format!("{base_url}/blob10m")).await {
            Ok(r) => println!("{{\"blob_bytes\":{},\"total_ms\":{},\"mb_per_s\":{:.3},\"sha256_ok\":{:?}}}", r.body_bytes, r.total_ms, r.mb_per_s, r.sha256_ok),
            Err(e) => println!("{{\"blob_error\":{:?}}}", e),
        }
    });
}
