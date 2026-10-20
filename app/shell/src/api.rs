//! Tipi che attraversano il confine Rust → JavaScript della finestra PRINCIPALE (I-31).
//!
//! Regola: i comandi della finestra principale restituiscono SOLO tipi definiti qui, e
//! qui non entrano chiavi, semi, parole, segreti di sessione o indirizzi onion interi.
//! Il test `inv-31-no-keys-in-js` controlla che `lib.rs` non esponga altro.
//! (Le parole del seme hanno un tipo a parte, in `api_seme.rs`, solo per la finestra del seme.)

use m4tr1x_identity::creazione::IdentitaPubblica;
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

/// Solo chiavi PUBBLICHE: identità (Nostr), DM, impronta del wallet.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Identita {
    pub identity: String,
    pub dm: String,
    pub wallet_impronta: String,
}

impl From<IdentitaPubblica> for Identita {
    fn from(i: IdentitaPubblica) -> Self {
        Self { identity: i.identity, dm: i.dm, wallet_impronta: i.wallet_impronta }
    }
}

/// Un post del feed, GIÀ verificato nel lato Rust (m4tr1x-feed → m4tr1x-protocol). L'autore è la sua
/// chiave PUBBLICA d'identità; il testo si mostra solo con `textContent`.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub id: String,
    pub autore: String,
    pub creato: u64,
    pub testo: String,
    /// nome dal profilo dell'autore, già pulito nel lato Rust (niente caratteri invisibili o di direzione)
    pub nome: Option<String>,
}

/// Profilo pubblico: SOLO nome e descrizione (decisione del fondatore, 20/10/2026; immagine in F6).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Profilo {
    pub nome: String,
    pub descrizione: String,
}

impl From<m4tr1x_protocol::Profilo> for Profilo {
    fn from(p: m4tr1x_protocol::Profilo) -> Self {
        Self { nome: p.name, descrizione: p.about }
    }
}

/// Il feed: post dal più recente, quanti eventi sono stati scartati, quanto ci è voluto.
/// Nessun indirizzo del nodo: l'onion resta nel lato Rust.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Feed {
    pub post: Vec<Post>,
    pub scartati: u32,
    pub secondi: u64,
    pub tentativi: u32,
}

impl From<m4tr1x_feed::Feed> for Feed {
    fn from(f: m4tr1x_feed::Feed) -> Self {
        Self {
            post: f.post.into_iter().map(|p| Post { id: p.id, autore: p.autore, creato: p.creato, testo: p.testo, nome: p.nome }).collect(),
            scartati: u32::try_from(f.scartati).unwrap_or(u32::MAX),
            secondi: f.secondi,
            tentativi: f.tentativi,
        }
    }
}

/// Dove si trova il feed, per non sembrare bloccati ("sto cercando il nodo… 47 s, tentativo 2").
/// `fase`: "fermo" | "tor" (tor non ancora pronto) | "cerco" | "scarico" | "invio" | "fatto" | "non_raggiungibile"
/// | "attesa_ripresa" (budget scaduto, Tor continua a cercare: `secondi` = quanti ne mancano alla ripresa).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Avanzamento {
    pub fase: String,
    pub secondi: u64,
    pub tentativo: u32,
}

impl Avanzamento {
    pub fn fermo() -> Self {
        Self { fase: "fermo".into(), secondi: 0, tentativo: 0 }
    }
    pub fn tor(secondi: u64) -> Self {
        Self { fase: "tor".into(), secondi, tentativo: 0 }
    }
}

impl From<&m4tr1x_feed::Fase> for Avanzamento {
    fn from(f: &m4tr1x_feed::Fase) -> Self {
        use m4tr1x_feed::Fase::*;
        let (fase, secondi, tentativo) = match *f {
            CercoIlNodo { secondi, tentativo } => ("cerco", secondi, tentativo),
            Scarico { secondi, tentativo } => ("scarico", secondi, tentativo),
            Invio { secondi, tentativo } => ("invio", secondi, tentativo),
            Fatto { secondi, tentativo } => ("fatto", secondi, tentativo),
            NonRaggiungibile { secondi, tentativo } => ("non_raggiungibile", secondi, tentativo),
            AttesaRipresa { fra_secondi } => ("attesa_ripresa", fra_secondi, 0),
        };
        Self { fase: fase.into(), secondi, tentativo }
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
