//! Persone che segui (I-70).
//!
//! L'elenco sta su questo dispositivo (file privato) e non si pubblica (niente kind 3). Il feed "Chi segui" lo manda
//! al nodo via Tor (`authors`) e ricontrolla la risposta: la privacy da proteggere è quella di chi scrive, non di chi
//! legge (fondatore, 26/11/2026).

use crate::bloccati::chiave_valida;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Mutex;

/// Persone al massimo nell'elenco.
pub const MASSIMO: usize = 10_000;

pub struct Seguiti {
    chiavi: Mutex<BTreeSet<String>>,
    /// None nelle prove: niente da scrivere su disco
    file: Option<PathBuf>,
}

impl Seguiti {
    /// Legge l'elenco dal file (una chiave per riga); righe non valide o oltre [`MASSIMO`] si ignorano.
    pub fn apri(file: Option<PathBuf>) -> Self {
        let chiavi = file
            .as_deref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map(|s| s.lines().map(str::trim).filter(|l| chiave_valida(l)).take(MASSIMO).map(String::from).collect())
            .unwrap_or_default();
        Self { chiavi: Mutex::new(chiavi), file }
    }

    pub fn contiene(&self, chiave: &str) -> bool {
        self.chiavi.lock().unwrap().contains(chiave)
    }

    pub fn elenco(&self) -> Vec<String> {
        self.chiavi.lock().unwrap().iter().cloned().collect()
    }

    /// Segui `chiave` (non te stesso).
    pub fn segui(&self, chiave: &str, mia: Option<&str>) -> Result<(), String> {
        let chiave = norma(chiave)?;
        if mia == Some(chiave.as_str()) {
            return Err("non puoi seguire te stesso".into());
        }
        let mut g = self.chiavi.lock().unwrap();
        if g.contains(&chiave) {
            return Ok(());
        }
        if g.len() >= MASSIMO {
            return Err(format!("segui già {MASSIMO} persone"));
        }
        g.insert(chiave.clone());
        if let Err(e) = self.salva(&g) {
            g.remove(&chiave);
            return Err(e);
        }
        Ok(())
    }

    /// Smetti di seguire `chiave`.
    pub fn smetti(&self, chiave: &str) -> Result<(), String> {
        let chiave = norma(chiave)?;
        let mut g = self.chiavi.lock().unwrap();
        if !g.remove(&chiave) {
            return Ok(());
        }
        if let Err(e) = self.salva(&g) {
            g.insert(chiave);
            return Err(e);
        }
        Ok(())
    }

    fn salva(&self, chiavi: &BTreeSet<String>) -> Result<(), String> {
        let Some(f) = &self.file else { return Ok(()) };
        let testo: String = chiavi.iter().map(|c| format!("{c}\n")).collect();
        crate::salva_privato(f, &testo).map_err(|e| format!("elenco di chi segui non salvato: {e}"))
    }

    /// Cancella l'elenco dal disco e dalla memoria (cancellazione dell'account).
    pub fn svuota(&self) -> Result<(), String> {
        if let Some(f) = &self.file {
            match std::fs::remove_file(f) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("elenco di chi segui non cancellato: {e}")),
            }
        }
        self.chiavi.lock().unwrap().clear();
        Ok(())
    }
}

fn norma(chiave: &str) -> Result<String, String> {
    let chiave = chiave.trim().to_ascii_lowercase();
    if !chiave_valida(&chiave) {
        return Err("chiave non valida: serve la chiave pubblica (64 caratteri)".into());
    }
    Ok(chiave)
}

#[cfg(test)]
mod prove {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn seguiti_solo_sul_dispositivo_e_te_stesso_no() {
        let dir = std::env::temp_dir().join(format!("m4tr1x-seguiti-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("seguiti");
        let _ = std::fs::remove_file(&f);
        let s = Seguiti::apri(Some(f.clone()));
        assert!(s.segui("non-una-chiave", None).is_err());
        assert!(s.segui(A, Some(A)).is_err(), "seguito te stesso");
        s.segui(&B.to_ascii_uppercase(), Some(A)).unwrap();
        assert!(s.contiene(B));
        // si rilegge dal disco, righe storte ignorate
        std::fs::write(&f, format!("{B}\nspazzatura\n{A}\n")).unwrap();
        let r = Seguiti::apri(Some(f.clone()));
        assert_eq!(r.elenco(), vec![A.to_string(), B.to_string()]);
        r.smetti(A).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), format!("{B}\n"));
        r.svuota().unwrap();
        assert!(!f.exists() && r.elenco().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
