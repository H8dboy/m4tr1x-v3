//! Persone bloccate (requisito degli store per i social; I-66).
//!
//! L'elenco sta SOLO su questo dispositivo, nella cartella dati dell'app, leggibile solo dall'utente: non si
//! pubblica mai (un elenco pubblico di chi blocchi direbbe a tutti con chi hai litigato, e chi sequestra un nodo
//! si porterebbe via una mappa dei conflitti). Si applica nel lato Rust, PRIMA che i contenuti arrivino
//! all'interfaccia: i post, le discussioni, le risposte, i file e i messaggi di chi blocchi non attraversano
//! nemmeno il confine verso il JavaScript.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Persone al massimo nell'elenco (un file più grande non si legge oltre).
pub const MASSIMO: usize = 10_000;

/// Una chiave pubblica d'identità: 64 caratteri esadecimali minuscoli.
pub fn chiave_valida(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

pub struct Bloccati {
    chiavi: Mutex<BTreeSet<String>>,
    /// None nelle prove: niente da scrivere su disco
    file: Option<PathBuf>,
}

impl Bloccati {
    /// Legge l'elenco dal file (una chiave per riga). Righe non valide e righe oltre [`MASSIMO`] si ignorano:
    /// un file rovinato non blocca l'app, al massimo blocca meno.
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

    /// L'elenco, in ordine.
    pub fn elenco(&self) -> Vec<String> {
        self.chiavi.lock().unwrap().iter().cloned().collect()
    }

    /// Blocca `chiave`. Bloccare la propria identità non ha senso: `mia` la esclude.
    pub fn blocca(&self, chiave: &str, mia: Option<&str>) -> Result<(), String> {
        let chiave = chiave.trim().to_ascii_lowercase();
        if !chiave_valida(&chiave) {
            return Err("chiave non valida: serve la chiave pubblica (64 caratteri)".into());
        }
        if mia == Some(chiave.as_str()) {
            return Err("non puoi bloccare te stesso".into());
        }
        let mut g = self.chiavi.lock().unwrap();
        if g.contains(&chiave) {
            return Ok(());
        }
        if g.len() >= MASSIMO {
            return Err(format!("hai già bloccato {MASSIMO} persone: sbloccane qualcuna"));
        }
        g.insert(chiave.clone());
        if let Err(e) = self.salva(&g) {
            g.remove(&chiave);
            return Err(e);
        }
        Ok(())
    }

    pub fn sblocca(&self, chiave: &str) -> Result<(), String> {
        let chiave = chiave.trim().to_ascii_lowercase();
        if !chiave_valida(&chiave) {
            return Err("chiave non valida: serve la chiave pubblica (64 caratteri)".into());
        }
        let mut g = self.chiavi.lock().unwrap();
        if !g.contains(&chiave) {
            return Ok(());
        }
        g.remove(&chiave);
        if let Err(e) = self.salva(&g) {
            g.insert(chiave);
            return Err(e);
        }
        Ok(())
    }

    /// Se il file non si scrive, chi chiama rimette l'elenco com'era (niente blocchi che spariscono al prossimo
    /// avvio senza dirlo).
    fn salva(&self, chiavi: &BTreeSet<String>) -> Result<(), String> {
        let Some(f) = &self.file else { return Ok(()) };
        let testo: String = chiavi.iter().map(|c| format!("{c}\n")).collect();
        crate::salva_privato(f, &testo).map_err(|e| format!("elenco dei bloccati non salvato: {e}"))
    }

    /// Cancella l'elenco dal disco e dalla memoria (cancellazione dell'account).
    pub fn svuota(&self) -> Result<(), String> {
        if let Some(f) = &self.file {
            match std::fs::remove_file(f) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("elenco dei bloccati non cancellato: {e}")),
            }
        }
        self.chiavi.lock().unwrap().clear();
        Ok(())
    }

    pub fn percorso(&self) -> Option<&Path> {
        self.file.as_deref()
    }
}

#[cfg(test)]
mod prove {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn cartella() -> PathBuf {
        let d = std::env::temp_dir().join(format!("m4tr1x-bloccati-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn blocca_sblocca_e_resta_dopo_il_riavvio() {
        let d = cartella();
        let f = d.join("bloccati");
        let b = Bloccati::apri(Some(f.clone()));
        b.blocca(A, None).unwrap();
        b.blocca(&B.to_ascii_uppercase(), None).unwrap(); // maiuscole accettate, salvate minuscole
        b.blocca(A, None).unwrap(); // due volte: innocuo
        assert!(b.contiene(A) && b.contiene(B));
        let dopo = Bloccati::apri(Some(f.clone()));
        assert_eq!(dopo.elenco(), vec![A.to_string(), B.to_string()]);
        dopo.sblocca(A).unwrap();
        assert_eq!(Bloccati::apri(Some(f.clone())).elenco(), vec![B.to_string()]);
        dopo.svuota().unwrap();
        assert!(!f.exists() && dopo.elenco().is_empty());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn chiavi_non_valide_e_se_stessi_rifiutati() {
        let b = Bloccati::apri(None);
        for brutta in ["", "abc", &"g".repeat(64), &"a".repeat(63), &"a".repeat(65), "../../etc/passwd"] {
            assert!(b.blocca(brutta, None).is_err(), "accettata: {brutta:?}");
            assert!(b.sblocca(brutta).is_err(), "accettata in sblocca: {brutta:?}");
        }
        assert!(b.blocca(A, Some(A)).is_err(), "bloccata la propria identità");
        assert!(b.elenco().is_empty());
    }

    #[test]
    fn file_rovinato_letto_a_meta_senza_errori() {
        let d = cartella();
        let f = d.join("bloccati");
        std::fs::write(&f, format!("spazzatura\n{A}\n\n  {B}  \nzz{A}\n")).unwrap();
        assert_eq!(Bloccati::apri(Some(f)).elenco(), vec![A.to_string(), B.to_string()]);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn mai_oltre_il_massimo() {
        let b = Bloccati::apri(None);
        for i in 0..MASSIMO {
            b.blocca(&format!("{i:064x}"), None).unwrap();
        }
        assert!(b.blocca(A, None).is_err());
        assert_eq!(b.elenco().len(), MASSIMO);
    }
}
