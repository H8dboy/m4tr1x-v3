// Elenco esplicito dei comandi dell'app (I-31): un comando Rust nuovo NON è
// chiamabile dal JavaScript finché non compare qui e nei permessi di
// capabilities/main.json. Così nessuna funzione finisce esposta per sbaglio.
const COMMANDS: &[&str] = &["stato", "identita", "apri_finestra_seme", "seme_nuovo", "seme_conferma", "seme_recupera"];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("build di tauri fallita");
}
