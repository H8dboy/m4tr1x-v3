//! Tipi che attraversano il confine Rust → JavaScript (I-31).
//!
//! Regola: ogni comando Tauri restituisce SOLO tipi definiti qui, e qui non
//! entrano chiavi, semi, segreti di sessione o indirizzi onion interi.
//! Il test `inv-31-no-keys-in-js` controlla che `lib.rs` non esponga altro.

use m4tr1x_transport::TransportStatus;
use serde::Serialize;

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Stato {
    pub versione: String,
    /// "spento" | "avvio" | "pronto" | "errore"
    pub trasporto: String,
    /// 0–100 durante l'avvio di Tor
    pub percento: u8,
}

impl Stato {
    pub fn new(versione: &str, t: &TransportStatus) -> Self {
        let (trasporto, percento) = match t {
            TransportStatus::Spento => ("spento", 0),
            TransportStatus::Avvio { percento } => ("avvio", *percento),
            TransportStatus::Pronto => ("pronto", 100),
            // il testo dell'errore resta nel lato Rust: può contenere percorsi locali
            TransportStatus::Errore(_) => ("errore", 0),
        };
        Self { versione: versione.to_string(), trasporto: trasporto.to_string(), percento }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_errore_del_trasporto_non_arriva_al_javascript() {
        let s = Stato::new("0.1.0", &TransportStatus::Errore("C:\\Users\\qualcuno\\tor.exe".into()));
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("Users"), "{json}");
        assert_eq!(s.trasporto, "errore");
    }
}
