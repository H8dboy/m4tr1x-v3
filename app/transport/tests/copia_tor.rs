//! I-37, copia verificata di tor (21/10/2026, decisioni del fondatore): ogni file si legge una volta e si
//! verifica in memoria; la copia sta in una cartella dell'app solo-proprietario (Linux 0700, Windows ACL
//! protetta con il solo utente); la vecchia copia si cancella senza seguire link o junction; tor parte
//! dalla copia, su Linux con l'ambiente pulito. Nessun tor vero: file finti con hash noti.

use m4tr1x_transport::tor_process::{cartella_binari, prepara_binari, rimuovi_senza_seguire};
use m4tr1x_transport::TorProcessConfig;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn h(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

fn tempdir(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("m4-copia-{nome}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Sorgente con un "tor" e due librerie finte; configurazione con i loro hash veri.
fn sorgente(nome: &str) -> (PathBuf, TorProcessConfig) {
    let src = tempdir(&format!("{nome}-src"));
    std::fs::write(src.join("tor"), b"tor finto").unwrap();
    std::fs::write(src.join("libuno.so"), b"libreria uno").unwrap();
    std::fs::write(src.join("libdue.so"), b"libreria due").unwrap();
    let cfg = TorProcessConfig {
        tor_bin: src.join("tor"),
        tor_sha256: h(b"tor finto"),
        librerie: vec![("libuno.so".into(), h(b"libreria uno")), ("libdue.so".into(), h(b"libreria due"))],
        data_dir: tempdir(&format!("{nome}-dati")),
        bootstrap_timeout: Duration::from_secs(5),
    };
    (src, cfg)
}

#[test]
fn copia_verificata_e_indipendente_dalla_sorgente() {
    let (src, cfg) = sorgente("copia");
    let tor = prepara_binari(&cfg).expect("copia");
    let dest = cartella_binari(&cfg.data_dir);
    assert_eq!(tor, dest.join("tor"));
    for (f, b) in [("tor", &b"tor finto"[..]), ("libuno.so", b"libreria uno"), ("libdue.so", b"libreria due")] {
        assert_eq!(std::fs::read(dest.join(f)).unwrap(), b, "{f} copiato male");
    }
    // la sorgente cambia DOPO la verifica: la copia no (si avvia la copia, non la sorgente)
    std::fs::write(src.join("tor"), b"sostituito").unwrap();
    assert_eq!(std::fs::read(&tor).unwrap(), b"tor finto");
    // un secondo avvio rifà la copia da capo: un file in più lasciato lì sparisce
    std::fs::write(src.join("tor"), b"tor finto").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::fs::write(dest.join("estraneo"), b"x").unwrap();
    prepara_binari(&cfg).expect("seconda copia");
    assert!(!dest.join("estraneo").exists(), "la vecchia copia non è stata cancellata");
}

#[test]
fn hash_sbagliato_o_libreria_mancante_niente_scritto() {
    let (src, mut cfg) = sorgente("sbagliato");
    cfg.librerie[1].1 = "00".repeat(32);
    let e = prepara_binari(&cfg).unwrap_err().to_string();
    assert!(e.contains("libdue.so") && e.contains("hash"), "errore inatteso: {e}");
    assert!(!cartella_binari(&cfg.data_dir).exists(), "con un hash sbagliato è stato scritto qualcosa");

    let (_, mut cfg) = sorgente("mancante");
    cfg.librerie.push(("libtre.so".into(), h(b"assente")));
    let e = prepara_binari(&cfg).unwrap_err().to_string();
    assert!(e.contains("libtre.so"), "errore inatteso: {e}");
    assert!(!cartella_binari(&cfg.data_dir).exists(), "con una libreria mancante è stato scritto qualcosa");

    let (_, mut cfg) = sorgente("tor");
    cfg.tor_sha256 = "00".repeat(32);
    assert!(prepara_binari(&cfg).is_err());
    assert!(!cartella_binari(&cfg.data_dir).exists());
    let _ = std::fs::remove_dir_all(src);
}

/// Collegamento (link a cartella su Linux, junction su Windows) da `link` a `bersaglio`.
fn collega(bersaglio: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(bersaglio, link).unwrap();
    #[cfg(windows)]
    {
        // junction: non servono privilegi (i link simbolici sì)
        let r = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(link).arg(bersaglio).output().unwrap();
        assert!(r.status.success(), "junction non creata: {}", String::from_utf8_lossy(&r.stderr));
    }
}

#[test]
fn la_pulizia_non_segue_link_o_junction() {
    // 1. tor-bin stessa è un collegamento verso una cartella fuori: si toglie il collegamento, il fuori resta
    let fuori = tempdir("fuori1");
    std::fs::write(fuori.join("canarino"), b"vivo").unwrap();
    let (_, cfg) = sorgente("link1");
    collega(&fuori, &cartella_binari(&cfg.data_dir));
    prepara_binari(&cfg).expect("copia sopra un collegamento");
    assert_eq!(std::fs::read(fuori.join("canarino")).unwrap(), b"vivo", "la pulizia ha seguito il collegamento");
    let m = std::fs::symlink_metadata(cartella_binari(&cfg.data_dir)).unwrap();
    assert!(m.is_dir() && !m.file_type().is_symlink(), "tor-bin non è una cartella vera");
    assert!(!fuori.join("tor").exists(), "la copia è finita nel bersaglio del collegamento");

    // 2. dentro una vecchia tor-bin vera c'è un collegamento verso fuori: il fuori resta
    let fuori2 = tempdir("fuori2");
    std::fs::write(fuori2.join("canarino"), b"vivo").unwrap();
    let (_, cfg) = sorgente("link2");
    let vecchia = cartella_binari(&cfg.data_dir);
    std::fs::create_dir_all(&vecchia).unwrap();
    collega(&fuori2, &vecchia.join("dentro"));
    prepara_binari(&cfg).expect("copia sopra una cartella con un collegamento");
    assert_eq!(std::fs::read(fuori2.join("canarino")).unwrap(), b"vivo", "la pulizia ha seguito il collegamento interno");

    // 3. la funzione da sola, su un collegamento diretto
    let fuori3 = tempdir("fuori3");
    std::fs::write(fuori3.join("canarino"), b"vivo").unwrap();
    let base = tempdir("base3");
    collega(&fuori3, &base.join("l"));
    rimuovi_senza_seguire(&base.join("l")).unwrap();
    assert!(std::fs::symlink_metadata(base.join("l")).is_err(), "collegamento non tolto");
    assert_eq!(std::fs::read(fuori3.join("canarino")).unwrap(), b"vivo");
    for d in [fuori, fuori2, fuori3, base] {
        let _ = std::fs::remove_dir_all(d);
    }
}

#[test]
fn copia_solo_proprietario() {
    let (_, cfg) = sorgente("permessi");
    prepara_binari(&cfg).unwrap();
    let dest = cartella_binari(&cfg.data_dir);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let modo = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(modo(&dest), 0o700, "cartella");
        assert_eq!(modo(&dest.join("tor")), 0o500, "tor");
        assert_eq!(modo(&dest.join("libuno.so")), 0o400, "libreria");
    }
    #[cfg(windows)]
    {
        let sid = m4tr1x_transport::tor_process::sid_utente().unwrap();
        for p in [dest.clone(), dest.join("tor"), dest.join("libuno.so")] {
            // ACL protetta (niente ereditato da sopra) e un solo SID: l'utente che esegue l'app
            let script = format!(
                "$a = Get-Acl -LiteralPath '{}'; \"$($a.AreAccessRulesProtected)\"; $a.Access | ForEach-Object {{ \"$($_.IdentityReference.Translate([System.Security.Principal.SecurityIdentifier]).Value) $($_.AccessControlType) $($_.IsInherited)\" }}",
                p.display().to_string().replace('\'', "''")
            );
            let r = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &script]).output().unwrap();
            assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
            let out = String::from_utf8_lossy(&r.stdout);
            let righe: Vec<&str> = out.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
            let e_cartella = p == dest;
            // la cartella ha la DACL protetta; i file ereditano SOLO da lei (che non eredita da nessuno)
            if e_cartella {
                assert_eq!(righe.first().copied(), Some("True"), "{}: ACL non protetta: {out}", p.display());
            }
            let voci = &righe[1..];
            assert!(!voci.is_empty(), "{}: nessuna voce nell'ACL", p.display());
            for v in voci {
                assert!(v.starts_with(&format!("{sid} Allow")), "{}: voce estranea nell'ACL: {v}", p.display());
            }
        }
    }
}

/// Linux: tor parte con l'ambiente pulito (solo LD_LIBRARY_PATH sulla copia). Un "tor" finto che è uno
/// script scrive il suo ambiente in un file e esce; l'avvio poi fallisce (non è tor), ma l'ambiente è scritto.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn su_linux_tor_parte_con_l_ambiente_pulito() {
    use m4tr1x_transport::{TorProcessTransport, Transport};
    let src = tempdir("ambiente-src");
    let uscita = tempdir("ambiente-uscita").join("env.txt");
    let script = format!("#!/bin/sh\n/usr/bin/env > '{}'\n", uscita.display());
    std::fs::write(src.join("tor"), &script).unwrap();
    std::fs::write(src.join("libuno.so"), b"libreria uno").unwrap();
    let cfg = TorProcessConfig {
        tor_bin: src.join("tor"),
        tor_sha256: h(script.as_bytes()),
        librerie: vec![("libuno.so".into(), h(b"libreria uno"))],
        data_dir: tempdir("ambiente-dati"),
        bootstrap_timeout: Duration::from_secs(5),
    };
    // variabili che NON devono arrivare a tor
    std::env::set_var("M4TR1X_SPIA", "1");
    std::env::set_var("LD_AUDIT", "/inesistente/spia.so");
    let t = TorProcessTransport::new(cfg.clone());
    let _ = t.avvia().await; // fallisce: lo script esce subito
    std::env::remove_var("LD_AUDIT");
    std::env::remove_var("M4TR1X_SPIA");
    let env = std::fs::read_to_string(&uscita).expect("il tor finto non è partito dalla copia");
    let voci: Vec<&str> = env.lines().filter(|l| !l.starts_with("PWD=") && !l.starts_with("SHLVL=") && !l.starts_with("_=")).collect();
    let attesa = format!("LD_LIBRARY_PATH={}", cartella_binari(&cfg.data_dir).display());
    assert_eq!(voci, vec![attesa.as_str()], "ambiente di tor non pulito: {env}");
}
