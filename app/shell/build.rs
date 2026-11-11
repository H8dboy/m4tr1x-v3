// Elenco esplicito dei comandi dell'app (I-31): un comando Rust nuovo NON è
// chiamabile dal JavaScript finché non compare qui e nei permessi di
// capabilities/main.json. Così nessuna funzione finisce esposta per sbaglio.
const COMMANDS: &[&str] = &["stato", "identita", "apri_finestra_seme", "seme_nuovo", "seme_conferma", "seme_recupera", "feed", "feed_avanzamento", "pubblica", "pubblica_avanzamento", "profilo_mio", "profilo_salva", "profilo_avanzamento", "messaggi", "messaggio_invia", "messaggi_avanzamento"];

fn main() {
    // Su Windows tauri-build incorpora il manifest (comctl32 v6) solo nell'eseguibile dell'app: senza,
    // le prove con il runtime finto (tests/i31_ipc.rs) non partono (STATUS_ENTRYPOINT_NOT_FOUND).
    // Stesso manifest di tauri-build 2.7.3, solo per gli eseguibili delle prove.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let m = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("tests").join("windows-test-manifest.xml");
        println!("cargo:rerun-if-changed={}", m.display());
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}", m.display());
    }
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("build di tauri fallita");
}
