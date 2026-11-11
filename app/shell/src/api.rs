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

/// Un file descritto da una scheda verificata (hash, tipo da un elenco chiuso, dimensione, sezione, titolo pulito).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct FileMedia {
    pub sha256: String,
    pub mime: String,
    pub dimensione: u64,
    pub sezione: String,
    pub titolo: String,
}

impl From<m4tr1x_protocol::SchedaFile> for FileMedia {
    fn from(f: m4tr1x_protocol::SchedaFile) -> Self {
        let sezione = match f.sezione {
            m4tr1x_protocol::Sezione::Signal => "signal",
            m4tr1x_protocol::Sezione::Film => "film",
            m4tr1x_protocol::Sezione::Musica => "musica",
        };
        Self { sha256: f.sha256, mime: f.mime, dimensione: f.dimensione, sezione: sezione.into(), titolo: f.titolo }
    }
}

/// Un reel, una foto, un film o un brano, GIÀ verificato. La didascalia si mostra solo con `textContent`.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Media {
    pub id: String,
    pub autore: String,
    pub nome: Option<String>,
    pub testo: String,
    pub creato: u64,
    pub file: FileMedia,
}

impl From<m4tr1x_feed::Media> for Media {
    fn from(m: m4tr1x_feed::Media) -> Self {
        Self { id: m.id, autore: m.autore, nome: m.nome, testo: m.testo, creato: m.creato, file: m.file.into() }
    }
}

/// Una sezione: dalla scheda più recente; `scartati` = schede non valide, contate e mai mostrate.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Galleria {
    pub media: Vec<Media>,
    pub scartati: u32,
}

impl From<m4tr1x_feed::Galleria> for Galleria {
    fn from(g: m4tr1x_feed::Galleria) -> Self {
        Self { media: g.media.into_iter().map(Media::from).collect(), scartati: u32::try_from(g.scartati).unwrap_or(u32::MAX) }
    }
}

/// Un file scelto e GIÀ pulito sul computer, pronto da pubblicare: tipo e dimensioni prima e dopo la pulizia.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct FilePronto {
    pub mime: String,
    pub dimensione: u64,
    pub prima: u64,
}

/// Una discussione del forum, GIÀ verificata nel lato Rust (m4tr1x-feed → m4tr1x-protocol). Titolo e testo
/// si mostrano solo con `textContent`.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Discussione {
    pub id: String,
    pub autore: String,
    pub nome: Option<String>,
    pub titolo: String,
    pub testo: String,
    pub creato: u64,
    pub risposte: u32,
    pub attivita: u64,
}

impl From<m4tr1x_feed::Discussione> for Discussione {
    fn from(d: m4tr1x_feed::Discussione) -> Self {
        Self { id: d.id, autore: d.autore, nome: d.nome, titolo: d.titolo, testo: d.testo, creato: d.creato, risposte: d.risposte, attivita: d.attivita }
    }
}

/// Il forum: discussioni dalla più attiva; `scartati` = eventi non validi, contati e mai mostrati.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Forum {
    pub discussioni: Vec<Discussione>,
    pub scartati: u32,
}

impl From<m4tr1x_feed::Forum> for Forum {
    fn from(f: m4tr1x_feed::Forum) -> Self {
        Self { discussioni: f.discussioni.into_iter().map(Discussione::from).collect(), scartati: u32::try_from(f.scartati).unwrap_or(u32::MAX) }
    }
}

/// Una discussione aperta con le sue risposte (dalla più vecchia).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct DiscussioneAperta {
    pub discussione: Discussione,
    pub risposte: Vec<Post>,
}

impl From<m4tr1x_feed::DiscussioneAperta> for DiscussioneAperta {
    fn from(d: m4tr1x_feed::DiscussioneAperta) -> Self {
        Self {
            discussione: d.discussione.into(),
            risposte: d.risposte.into_iter().map(|p| Post { id: p.id, autore: p.autore, creato: p.creato, testo: p.testo, nome: p.nome }).collect(),
        }
    }
}

/// Un messaggio privato, GIÀ aperto e controllato nel lato Rust (m4tr1x-posta → m4tr1x-dm). Il testo si
/// mostra solo con `textContent`.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Messaggio {
    /// l'ho scritto io
    pub mio: bool,
    pub testo: String,
    pub creato: u64,
}

impl From<m4tr1x_posta::Messaggio> for Messaggio {
    fn from(m: m4tr1x_posta::Messaggio) -> Self {
        Self { mio: m.mio, testo: m.testo, creato: m.creato }
    }
}

/// I messaggi scambiati con una persona. `identita` (pubblica) c'è SOLO se il suo annuncio rivendica con
/// la prova di possesso l'indirizzo dei messaggi `dm` (pubblico, I-60): senza, è un mittente sconosciuto.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Conversazione {
    pub identita: Option<String>,
    pub dm: String,
    pub nome: Option<String>,
    pub messaggi: Vec<Messaggio>,
}

/// La posta: conversazioni dalla più recente; `scartate` = buste non valide, contate e mai mostrate (I-61).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Posta {
    pub conversazioni: Vec<Conversazione>,
    pub scartate: u32,
}

impl From<m4tr1x_posta::Posta> for Posta {
    fn from(p: m4tr1x_posta::Posta) -> Self {
        Self {
            conversazioni: p
                .conversazioni
                .into_iter()
                .map(|c| Conversazione { identita: c.identita, dm: c.dm, nome: c.nome, messaggi: c.messaggi.into_iter().map(Messaggio::from).collect() })
                .collect(),
            scartate: u32::try_from(p.scartate).unwrap_or(u32::MAX),
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
