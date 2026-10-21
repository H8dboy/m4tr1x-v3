//! Configurazione di tor per l'APP (F4, passo 3; desktop intero dal 21/10/2026): hash solo dal lock della
//! piattaforma incluso alla compilazione, nessun tor se non indicato. La parte con tor vero usa il tor
//! estratto dal bundle del lock (`M4TR1X_TOR_DEL_LOCK`): se la variabile c'è e il tor non è quello del
//! lock, la prova FALLISCE (prima diceva "saltato" e passava: in CI su Linux succedeva sempre).

use m4tr1x_transport::tor_process::{cartella_binari, leggi_lock, lock_tor, sha256_file, FILE_LOCK};
use m4tr1x_transport::{TorProcessConfig, TorProcessTransport, Transport, TransportStatus};
use std::path::PathBuf;

fn lock_su_disco() -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join(FILE_LOCK.expect("piattaforma senza lock di tor"));
    std::fs::read_to_string(p).unwrap()
}

#[test]
fn gli_hash_inclusi_sono_quelli_del_lock() {
    assert_eq!(lock_tor().expect("lock incluso"), leggi_lock(&lock_su_disco()));
    #[cfg(target_os = "linux")]
    {
        let (_, librerie) = lock_tor().unwrap();
        let nomi: Vec<&str> = librerie.iter().map(|(n, _)| n.as_str()).collect();
        for l in ["libcrypto.so.3", "libssl.so.3", "libevent-2.1.so.7"] {
            assert!(nomi.contains(&l), "su Linux il lock deve fissare anche {l}");
        }
    }
}

#[test]
fn senza_tor_indicato_nessuna_configurazione() {
    let dati = std::env::temp_dir();
    assert!(TorProcessConfig::per_l_app(None, dati.clone()).is_none());
    assert!(TorProcessConfig::per_l_app(Some(PathBuf::new()), dati).is_none());
}

#[test]
fn l_app_usa_solo_gli_hash_del_lock() {
    let c = TorProcessConfig::per_l_app(Some(PathBuf::from("qualsiasi/tor.exe")), std::env::temp_dir()).unwrap();
    let (tor, librerie) = lock_tor().unwrap();
    assert_eq!(c.tor_sha256, tor);
    assert_eq!(c.librerie, librerie);
}

/// Un tor con un hash diverso da quello del lock: l'app non lo avvia (stato di errore, nessun processo).
#[tokio::test]
async fn tor_con_hash_diverso_non_parte() {
    let cartella = tempdir("m4-tor-falso");
    let falso = cartella.join("tor.exe");
    std::fs::write(&falso, b"non sono tor").unwrap();
    let dati = tempdir("m4-app-falso");
    let t = TorProcessTransport::new(TorProcessConfig::per_l_app(Some(falso.clone()), dati.clone()).unwrap());
    assert!(t.avvia().await.is_err());
    assert!(matches!(t.status(), TransportStatus::Errore(_)), "{:?}", t.status());
    assert!(t.tor_pid().await.is_none());
    assert!(!cartella_binari(&dati).exists(), "copia creata per un tor rifiutato");
    let _ = std::fs::remove_dir_all(cartella);
    let _ = std::fs::remove_dir_all(dati);
}

/// Con il tor del lock (`M4TR1X_TOR_DEL_LOCK`, estratto dal bundle verificato): l'app lo avvia dalla SUA
/// copia verificata, nella SUA cartella dati. Su Linux servono anche le librerie del bundle accanto.
#[tokio::test]
async fn con_il_tor_del_lock_l_app_lo_avvia_dalla_sua_copia() {
    let Some(bin) = std::env::var_os("M4TR1X_TOR_DEL_LOCK").map(PathBuf::from) else {
        eprintln!("SALTATO: serve M4TR1X_TOR_DEL_LOCK");
        return;
    };
    assert_eq!(sha256_file(&bin).ok(), Some(lock_tor().unwrap().0), "M4TR1X_TOR_DEL_LOCK non è il tor del lock");
    let dati = tempdir("m4-app-vero");
    let t = TorProcessTransport::new(TorProcessConfig::per_l_app(Some(bin), dati.clone()).unwrap());
    t.avvia().await.unwrap();
    let pid = t.tor_pid().await.expect("tor non partito");
    assert!(dati.join("tor").is_dir(), "tor non usa la cartella dati dell'app");
    assert!(cartella_binari(&dati).is_dir(), "copia verificata assente");
    #[cfg(target_os = "linux")]
    {
        // il processo che gira è la COPIA, non il file indicato
        let exe = std::fs::read_link(format!("/proc/{pid}/exe")).unwrap();
        assert_eq!(exe, cartella_binari(&dati).canonicalize().unwrap().join("tor"), "tor non è partito dalla copia verificata");
    }
    #[cfg(windows)]
    {
        let r = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &format!("(Get-Process -Id {pid}).Path")])
            .output()
            .unwrap();
        let exe = PathBuf::from(String::from_utf8_lossy(&r.stdout).trim());
        assert_eq!(exe.canonicalize().unwrap(), cartella_binari(&dati).join("tor.exe").canonicalize().unwrap(), "tor non è partito dalla copia verificata");
    }
    t.shutdown().await;
    assert_eq!(t.status(), TransportStatus::Spento);
    let _ = std::fs::remove_dir_all(dati);
}

fn tempdir(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("{nome}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&d).unwrap();
    d
}
