//! Configurazione di tor per l'APP (F4, passo 3): hash solo dal lock incluso alla compilazione,
//! nessun tor se non indicato. La parte con tor vero gira solo con il tor.exe del lock (Windows):
//! i test di I-37 in CI usano il tor di Ubuntu, che ha un altro hash e l'app giustamente rifiuta.

use m4tr1x_transport::tor_process::{sha256_file, sha256_tor_windows};
use m4tr1x_transport::{TorProcessConfig, TorProcessTransport, Transport, TransportStatus};
use std::path::PathBuf;

fn lock() -> serde_json::Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("tor-windows.lock.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

#[test]
fn l_hash_incluso_e_quello_del_lock() {
    assert_eq!(sha256_tor_windows(), lock()["tor_exe_sha256"].as_str().unwrap());
}

#[test]
fn senza_tor_indicato_nessuna_configurazione() {
    let dati = std::env::temp_dir();
    assert!(TorProcessConfig::per_l_app(None, dati.clone()).is_none());
    assert!(TorProcessConfig::per_l_app(Some(PathBuf::new()), dati).is_none());
}

#[test]
fn l_app_usa_solo_l_hash_del_lock() {
    let c = TorProcessConfig::per_l_app(Some(PathBuf::from("qualsiasi/tor.exe")), std::env::temp_dir()).unwrap();
    assert_eq!(c.tor_sha256, sha256_tor_windows());
}

/// Un tor con un hash diverso da quello del lock: l'app non lo avvia (stato di errore, nessun processo).
#[tokio::test]
async fn tor_con_hash_diverso_non_parte() {
    let falso = std::env::temp_dir().join(format!("m4-tor-falso-{}.exe", std::process::id()));
    std::fs::write(&falso, b"non sono tor").unwrap();
    let dati = tempdir("m4-app-falso");
    let t = TorProcessTransport::new(TorProcessConfig::per_l_app(Some(falso.clone()), dati.clone()).unwrap());
    assert!(t.avvia().await.is_err());
    assert!(matches!(t.status(), TransportStatus::Errore(_)), "{:?}", t.status());
    assert!(t.tor_pid().await.is_none());
    let _ = std::fs::remove_file(falso);
    let _ = std::fs::remove_dir_all(dati);
}

/// Con il tor.exe del lock (M4TR1X_TOR_BIN su Windows): l'app lo avvia nella SUA cartella dati.
#[tokio::test]
async fn con_il_tor_del_lock_l_app_lo_avvia_nella_sua_cartella() {
    let Some(bin) = std::env::var_os("M4TR1X_TOR_BIN").map(PathBuf::from) else {
        eprintln!("SALTATO: serve M4TR1X_TOR_BIN");
        return;
    };
    if sha256_file(&bin).ok().as_deref() != Some(sha256_tor_windows().as_str()) {
        eprintln!("SALTATO: questo tor non è quello del lock (es. tor di Ubuntu in CI)");
        return;
    }
    let dati = tempdir("m4-app-vero");
    let t = TorProcessTransport::new(TorProcessConfig::per_l_app(Some(bin), dati.clone()).unwrap());
    t.avvia().await.unwrap();
    assert!(t.tor_pid().await.is_some(), "tor non partito");
    assert!(dati.join("tor").is_dir(), "tor non usa la cartella dati dell'app");
    t.shutdown().await;
    assert_eq!(t.status(), TransportStatus::Spento);
    let _ = std::fs::remove_dir_all(dati);
}

fn tempdir(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("{nome}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&d).unwrap();
    d
}
