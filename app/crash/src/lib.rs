//! Registro dei crash dell'app (decisione del fondatore, 20/10/2026: il primo avvio dell'app vera si è
//! chiuso da solo senza lasciare traccia). Un panic finisce in `crash.log` nella cartella dati dell'app.
//!
//! SENZA SEGRETI, per costruzione:
//!   - si scrivono ora, versione, thread, file:riga del panic e la pila (nomi di funzione, niente dati);
//!   - il messaggio del panic si scrive SOLO se è un testo fisso del codice (`&'static str`, deciso alla
//!     compilazione: non può contenere parole del seme, chiavi, onion o percorsi dell'utente);
//!   - un messaggio costruito mentre l'app gira (`format!`, `unwrap` su un errore) NON si scrive: può
//!     contenere qualsiasi dato. Restano la lunghezza e il punto del codice, che bastano a cercarlo.
//! Il file non supera [`MASSIMO_BYTE`]: oltre, si tiene solo l'ultimo crash.
//! Dopo la scrittura si chiama l'hook di prima (quello standard stampa sulla console in debug).

use std::fmt::Write as _;
use std::io::Write as _;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};

pub const NOME_FILE: &str = "crash.log";
/// Dimensione massima del registro: oltre, si riparte dall'ultimo crash.
pub const MASSIMO_BYTE: u64 = 256 * 1024;

/// Installa il registro dei crash su `<cartella>/crash.log`. Si può chiamare di nuovo con una cartella
/// diversa (es. quella definitiva data da Tauri): vale l'ultima, e l'hook precedente resta in coda.
pub fn installa(cartella: PathBuf, versione: &'static str) {
    let precedente = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // un errore nello scrivere il registro non deve mai coprire il crash vero
        let _ = scrivi(&cartella, &voce(info, versione));
        precedente(info);
    }));
}

/// Cartella dati dell'app prima che Tauri parta (stessa regola di `app_data_dir` di Tauri:
/// `<dati utente>/<identificatore>`), così si registrano anche i crash dell'avvio.
pub fn cartella_predefinita(identificatore: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Application Support"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")));
    base.map(|b| b.join(identificatore))
}

/// Il testo di una voce del registro.
fn voce(info: &PanicHookInfo<'_>, versione: &str) -> String {
    let mut s = String::new();
    let secondi = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let _ = writeln!(s, "=== crash · unix {secondi} · M4TR1X {versione} · {} {}", std::env::consts::OS, std::env::consts::ARCH);
    let _ = writeln!(s, "thread: {}", std::thread::current().name().unwrap_or("(senza nome)"));
    match info.location() {
        Some(l) => {
            let _ = writeln!(s, "punto: {}:{}:{}", l.file(), l.line(), l.column());
        }
        None => s.push_str("punto: sconosciuto\n"),
    }
    let _ = writeln!(s, "messaggio: {}", messaggio_sicuro(info.payload()));
    let _ = writeln!(s, "pila:\n{}", std::backtrace::Backtrace::force_capture());
    s
}

/// Solo i testi fissi del codice; quelli costruiti a runtime si omettono (possono contenere dati).
fn messaggio_sicuro(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(fisso) = payload.downcast_ref::<&'static str>() {
        (*fisso).to_owned()
    } else if let Some(costruito) = payload.downcast_ref::<String>() {
        format!("(costruito a runtime, {} caratteri: omesso perché può contenere dati dell'utente)", costruito.chars().count())
    } else {
        "(tipo non testuale: omesso)".to_owned()
    }
}

fn scrivi(cartella: &Path, testo: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(cartella)?;
    let p = cartella.join(NOME_FILE);
    let troppo = std::fs::metadata(&p).map(|m| m.len() + testo.len() as u64 > MASSIMO_BYTE).unwrap_or(false);
    let mut f = std::fs::OpenOptions::new().create(true).write(true).append(!troppo).truncate(troppo).open(p)?;
    f.write_all(testo.as_bytes())?;
    f.sync_all()
}
