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
    /// bandierine della moderazione del nodo (I-65): con almeno una, l'interfaccia copre il testo con un velo
    pub bandierine: Vec<Bandierina>,
    /// persone diverse che hanno messo "mi piace" (I-69)
    pub mi_piace: u32,
    /// la TUA reazione, se c'è (id dell'evento, per toglierla)
    pub mio_mi_piace: Option<String>,
    /// se la voce è una ricondivisione (I-73): chi l'ha fatta e l'id della ricondivisione (per toglierla se è tua)
    pub ricondiviso_da: Option<String>,
    pub ricondivisione: Option<String>,
}

/// Una notifica (I-71): `tipo` = "mi_piace" o "risposta"; chi, su quale tuo contenuto, quando. Niente testo.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Notifica {
    pub tipo: String,
    pub id: String,
    pub autore: String,
    pub contenuto: String,
    pub creato: u64,
    /// arrivata dopo l'ultima volta che le hai guardate
    pub nuova: bool,
}

impl Notifica {
    pub fn da(n: m4tr1x_feed::Notifica, vista_fino: u64) -> Self {
        let tipo = match n.tipo {
            m4tr1x_feed::TipoNotifica::MiPiace => "mi_piace",
            m4tr1x_feed::TipoNotifica::Risposta => "risposta",
        };
        Self { tipo: tipo.into(), nuova: n.creato > vista_fino, id: n.id, autore: n.autore, contenuto: n.contenuto, creato: n.creato }
    }
}

/// Reazioni per id di contenuto, come le conta m4tr1x-feed.
pub type Reazioni = std::collections::HashMap<String, m4tr1x_feed::Reazioni>;

/// (quante, mia) per il contenuto `id`.
fn reazioni_di(r: &Reazioni, id: &str) -> (u32, Option<String>) {
    r.get(id).map(|x| (x.quante, x.mia.clone())).unwrap_or((0, None))
}

/// Bandierine per id di contenuto, come arrivano dal nodo (m4tr1x-feed, già controllate dal trasporto).
pub type Veli = std::collections::HashMap<String, Vec<m4tr1x_feed::Bandierina>>;

/// Le bandierine del contenuto `id` (nessuna se il nodo non ne ha date per lui).
fn bandierine_di(b: &Veli, id: &str) -> Vec<Bandierina> {
    b.get(id).map(|l| l.iter().map(Bandierina::from).collect()).unwrap_or_default()
}

/// Una bandierina della moderazione (I-65): solo categoria (da un elenco chiuso), peso e «scritto in codice».
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Bandierina {
    pub categoria: String,
    pub peso: u8,
    pub codice: bool,
}

impl From<&m4tr1x_feed::Bandierina> for Bandierina {
    fn from(b: &m4tr1x_feed::Bandierina) -> Self {
        Self { categoria: b.categoria.to_string(), peso: b.peso, codice: b.codice }
    }
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
    /// le bandierine sono arrivate dal nodo? Se no, i post si vedono senza veli e l'interfaccia lo dice
    pub bandierine_disponibili: bool,
}

impl Feed {
    /// Attacca a ogni post i suoi "mi piace" (I-69). `None`: il nodo non li ha dati (restano a zero).
    pub fn con_reazioni(mut self, r: Option<&Reazioni>) -> Self {
        if let Some(r) = r {
            for p in &mut self.post {
                (p.mi_piace, p.mio_mi_piace) = reazioni_di(r, &p.id);
            }
        }
        self
    }

    /// Attacca a ogni post le sue bandierine (I-65). `None`: il nodo non le ha date.
    pub fn con_bandierine(mut self, b: Option<&std::collections::HashMap<String, Vec<m4tr1x_feed::Bandierina>>>) -> Self {
        self.bandierine_disponibili = b.is_some();
        if let Some(b) = b {
            for p in &mut self.post {
                p.bandierine = b.get(&p.id).map(|l| l.iter().map(Bandierina::from).collect()).unwrap_or_default();
            }
        }
        self
    }
}

impl From<m4tr1x_feed::Feed> for Feed {
    fn from(f: m4tr1x_feed::Feed) -> Self {
        Self {
            post: f
                .post
                .into_iter()
                .map(|p| {
                    let (ricondiviso_da, ricondivisione) = p.ricondivisione.map(|r| (Some(r.da), Some(r.id))).unwrap_or((None, None));
                    Post { id: p.id, autore: p.autore, creato: p.creato, testo: p.testo, nome: p.nome, bandierine: vec![], mi_piace: 0, mio_mi_piace: None, ricondiviso_da, ricondivisione }
                })
                .collect(),
            scartati: u32::try_from(f.scartati).unwrap_or(u32::MAX),
            secondi: f.secondi,
            tentativi: f.tentativi,
            bandierine_disponibili: false,
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
    /// hash dei pezzi in ordine (vuoto = un blob solo): l'interfaccia li ripassa a `media_file` così come sono
    pub pezzi: Vec<String>,
    /// solo per le foto e i reel a celle: i byte di ogni pezzo, nello stesso ordine (l'interfaccia li ripassa a
    /// `celle_apri` così come sono); vuoto per gli altri tipi
    pub misure: Vec<u64>,
    /// foto o reel a celle (alveare): si apre con `celle_apri`, non con `media_file`
    pub celle: bool,
}

impl From<m4tr1x_protocol::SchedaFile> for FileMedia {
    fn from(f: m4tr1x_protocol::SchedaFile) -> Self {
        let sezione = match f.sezione {
            m4tr1x_protocol::Sezione::Signal => "signal",
            m4tr1x_protocol::Sezione::Film => "film",
            m4tr1x_protocol::Sezione::Musica => "musica",
        };
        let celle = f.a_celle();
        Self { sha256: f.sha256, mime: f.mime, dimensione: f.dimensione, sezione: sezione.into(), titolo: f.titolo, pezzi: f.pezzi, misure: f.misure, celle }
    }
}

/// Una foto o un reel appena codificati a celle sul computer, pronti da pubblicare: quanti pezzi, quanti byte in
/// tutto e quanti nel primo (l'anteprima), fotogrammi (0 per una foto).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct CellePronte {
    pub mime: String,
    pub pezzi: u32,
    pub dimensione: u64,
    pub anteprima: u64,
    pub fotogrammi: u32,
    pub millisecondi: u64,
}

/// Dove sta la foto o il reel a celle che si sta guardando: pezzi arrivati su quanti, byte scesi, fotogrammi
/// decodificati finora, fotogrammi al secondo, errore se qualcosa è andato storto. I pixel si chiedono a parte
/// (`celle_quadro`), grezzi.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct CelleStato {
    pub larghezza: u32,
    pub altezza: u32,
    pub pezzi_arrivati: u32,
    pub pezzi_totali: u32,
    pub byte_arrivati: u64,
    pub fotogrammi: u32,
    pub fps: u8,
    pub finito: bool,
    pub errore: Option<String>,
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
    /// bandierine della moderazione del nodo (I-65): con almeno una, la tessera resta coperta da un velo
    pub bandierine: Vec<Bandierina>,
    /// persone diverse che hanno messo "mi piace" (I-69), e la TUA reazione
    pub mi_piace: u32,
    pub mio_mi_piace: Option<String>,
}

impl From<m4tr1x_feed::Media> for Media {
    fn from(m: m4tr1x_feed::Media) -> Self {
        Self { id: m.id, autore: m.autore, nome: m.nome, testo: m.testo, creato: m.creato, file: m.file.into(), bandierine: vec![], mi_piace: 0, mio_mi_piace: None }
    }
}

/// Una sezione: dalla scheda più recente; `scartati` = schede non valide, contate e mai mostrate.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Galleria {
    pub media: Vec<Media>,
    pub scartati: u32,
    /// le bandierine sono arrivate dal nodo? (I-65)
    pub bandierine_disponibili: bool,
}

impl Galleria {
    /// Attacca a ogni scheda i suoi "mi piace" (I-69).
    pub fn con_reazioni(mut self, r: Option<&Reazioni>) -> Self {
        if let Some(r) = r {
            for m in &mut self.media {
                (m.mi_piace, m.mio_mi_piace) = reazioni_di(r, &m.id);
            }
        }
        self
    }

    /// Attacca a ogni scheda le sue bandierine (I-65). `None`: il nodo non le ha date.
    pub fn con_bandierine(mut self, b: Option<&Veli>) -> Self {
        self.bandierine_disponibili = b.is_some();
        if let Some(b) = b {
            for m in &mut self.media {
                m.bandierine = bandierine_di(b, &m.id);
            }
        }
        self
    }
}

impl From<m4tr1x_feed::Galleria> for Galleria {
    fn from(g: m4tr1x_feed::Galleria) -> Self {
        Self { media: g.media.into_iter().map(Media::from).collect(), scartati: u32::try_from(g.scartati).unwrap_or(u32::MAX), bandierine_disponibili: false }
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
    /// bandierine della moderazione del nodo (I-65), sul titolo o sul testo
    pub bandierine: Vec<Bandierina>,
}

impl From<m4tr1x_feed::Discussione> for Discussione {
    fn from(d: m4tr1x_feed::Discussione) -> Self {
        Self { id: d.id, autore: d.autore, nome: d.nome, titolo: d.titolo, testo: d.testo, creato: d.creato, risposte: d.risposte, attivita: d.attivita, bandierine: vec![] }
    }
}

/// Il forum: discussioni dalla più attiva; `scartati` = eventi non validi, contati e mai mostrati.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Forum {
    pub discussioni: Vec<Discussione>,
    pub scartati: u32,
    /// le bandierine sono arrivate dal nodo? (I-65)
    pub bandierine_disponibili: bool,
}

impl Forum {
    /// Attacca a ogni discussione le sue bandierine (I-65). `None`: il nodo non le ha date.
    pub fn con_bandierine(mut self, b: Option<&Veli>) -> Self {
        self.bandierine_disponibili = b.is_some();
        if let Some(b) = b {
            for d in &mut self.discussioni {
                d.bandierine = bandierine_di(b, &d.id);
            }
        }
        self
    }
}

impl From<m4tr1x_feed::Forum> for Forum {
    fn from(f: m4tr1x_feed::Forum) -> Self {
        Self { discussioni: f.discussioni.into_iter().map(Discussione::from).collect(), scartati: u32::try_from(f.scartati).unwrap_or(u32::MAX), bandierine_disponibili: false }
    }
}

/// Una discussione aperta con le sue risposte (dalla più vecchia).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct DiscussioneAperta {
    pub discussione: Discussione,
    pub risposte: Vec<Post>,
    /// le bandierine sono arrivate dal nodo? (I-65)
    pub bandierine_disponibili: bool,
}

impl DiscussioneAperta {
    /// Bandierine sulla discussione e su ogni risposta (I-65). `None`: il nodo non le ha date.
    pub fn con_bandierine(mut self, b: Option<&Veli>) -> Self {
        self.bandierine_disponibili = b.is_some();
        if let Some(b) = b {
            self.discussione.bandierine = bandierine_di(b, &self.discussione.id);
            for r in &mut self.risposte {
                r.bandierine = bandierine_di(b, &r.id);
            }
        }
        self
    }

    /// Attacca a ogni risposta i suoi "mi piace" (I-69).
    pub fn con_reazioni(mut self, r: Option<&Reazioni>) -> Self {
        if let Some(r) = r {
            for p in &mut self.risposte {
                (p.mi_piace, p.mio_mi_piace) = reazioni_di(r, &p.id);
            }
        }
        self
    }

    /// Gli id di cui chiedere le bandierine: la discussione e le sue risposte.
    pub fn ids(&self) -> Vec<String> {
        std::iter::once(self.discussione.id.clone()).chain(self.risposte.iter().map(|r| r.id.clone())).collect()
    }
}

impl From<m4tr1x_feed::DiscussioneAperta> for DiscussioneAperta {
    fn from(d: m4tr1x_feed::DiscussioneAperta) -> Self {
        Self {
            discussione: d.discussione.into(),
            risposte: d.risposte.into_iter().map(|p| Post { id: p.id, autore: p.autore, creato: p.creato, testo: p.testo, nome: p.nome, bandierine: vec![], mi_piace: 0, mio_mi_piace: None, ricondiviso_da: None, ricondivisione: None }).collect(),
            bandierine_disponibili: false,
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
    /// file a pezzi: quanti pezzi sono passati su quanti (0/0 fuori da un trasferimento a pezzi)
    pub pezzi_fatti: u32,
    pub pezzi_totali: u32,
}

impl Avanzamento {
    pub fn fermo() -> Self {
        Self { fase: "fermo".into(), secondi: 0, tentativo: 0, pezzi_fatti: 0, pezzi_totali: 0 }
    }
    pub fn tor(secondi: u64) -> Self {
        Self { fase: "tor".into(), secondi, tentativo: 0, pezzi_fatti: 0, pezzi_totali: 0 }
    }
}

impl From<&m4tr1x_feed::Fase> for Avanzamento {
    fn from(f: &m4tr1x_feed::Fase) -> Self {
        use m4tr1x_feed::Fase::*;
        let (fase, secondi, tentativo, pezzi) = match *f {
            CercoIlNodo { secondi, tentativo } => ("cerco", secondi, tentativo, (0, 0)),
            Scarico { secondi, tentativo } => ("scarico", secondi, tentativo, (0, 0)),
            Invio { secondi, tentativo } => ("invio", secondi, tentativo, (0, 0)),
            Fatto { secondi, tentativo } => ("fatto", secondi, tentativo, (0, 0)),
            NonRaggiungibile { secondi, tentativo } => ("non_raggiungibile", secondi, tentativo, (0, 0)),
            AttesaRipresa { fra_secondi } => ("attesa_ripresa", fra_secondi, 0, (0, 0)),
            Pezzo { fatti, totali, secondi, verso_il_nodo: true } => ("carico_pezzo", secondi, 0, (fatti, totali)),
            Pezzo { fatti, totali, secondi, verso_il_nodo: false } => ("scarico_pezzo", secondi, 0, (fatti, totali)),
        };
        Self { fase: fase.into(), secondi, tentativo, pezzi_fatti: pezzi.0, pezzi_totali: pezzi.1 }
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

#[cfg(test)]
mod prove_bandierine {
    use super::*;

    fn post(id: &str) -> Post {
        Post { id: id.repeat(64), autore: "f".repeat(64), creato: 1, testo: "ciao".into(), nome: None, bandierine: vec![], mi_piace: 0, mio_mi_piace: None, ricondiviso_da: None, ricondivisione: None }
    }

    #[test]
    fn con_bandierine_attacca_solo_quelle_del_post_giusto() {
        let feed = Feed { post: vec![post("a"), post("b")], scartati: 0, secondi: 1, tentativi: 1, bandierine_disponibili: false };
        let mut b = std::collections::HashMap::new();
        b.insert("a".repeat(64), vec![m4tr1x_feed::Bandierina { categoria: "truffa", peso: 3, codice: true }]);
        let f = feed.clone().con_bandierine(Some(&b));
        assert!(f.bandierine_disponibili);
        assert_eq!(f.post[0].bandierine, vec![Bandierina { categoria: "truffa".into(), peso: 3, codice: true }]);
        assert!(f.post[1].bandierine.is_empty());
        // il nodo non le ha date: niente veli, e si sa
        let f = feed.con_bandierine(None);
        assert!(!f.bandierine_disponibili && f.post.iter().all(|p| p.bandierine.is_empty()));
    }

    #[test]
    fn forum_discussioni_risposte_e_schede_ricevono_solo_le_loro() {
        let mut b = Veli::new();
        b.insert("a".repeat(64), vec![m4tr1x_feed::Bandierina { categoria: "spam", peso: 1, codice: false }]);
        b.insert("c".repeat(64), vec![m4tr1x_feed::Bandierina { categoria: "odio", peso: 2, codice: true }]);
        let disc = |id: &str| Discussione { id: id.repeat(64), autore: "f".repeat(64), nome: None, titolo: "t".into(), testo: "x".into(), creato: 1, risposte: 0, attivita: 1, bandierine: vec![] };
        let f = Forum { discussioni: vec![disc("a"), disc("b")], scartati: 0, bandierine_disponibili: false }.con_bandierine(Some(&b));
        assert!(f.bandierine_disponibili);
        assert_eq!(f.discussioni[0].bandierine[0].categoria, "spam");
        assert!(f.discussioni[1].bandierine.is_empty());
        let aperta = DiscussioneAperta { discussione: disc("b"), risposte: vec![post("c"), post("d")], bandierine_disponibili: false };
        assert_eq!(aperta.ids(), vec!["b".repeat(64), "c".repeat(64), "d".repeat(64)]);
        let aperta = aperta.con_bandierine(Some(&b));
        assert!(aperta.discussione.bandierine.is_empty());
        assert_eq!(aperta.risposte[0].bandierine[0].categoria, "odio");
        assert!(aperta.risposte[1].bandierine.is_empty());
        let scheda = |id: &str| Media {
            id: id.repeat(64), autore: "f".repeat(64), nome: None, testo: "x".into(), creato: 1, bandierine: vec![], mi_piace: 0, mio_mi_piace: None,
            file: FileMedia { sha256: "e".repeat(64), mime: "video/mp4".into(), dimensione: 1, sezione: "signal".into(), titolo: "t".into(), pezzi: vec![], misure: vec![], celle: false },
        };
        let g = Galleria { media: vec![scheda("b"), scheda("a")], scartati: 0, bandierine_disponibili: false }.con_bandierine(Some(&b));
        assert!(g.media[0].bandierine.is_empty());
        assert_eq!(g.media[1].bandierine[0].categoria, "spam");
        assert!(!Galleria { media: vec![], scartati: 0, bandierine_disponibili: true }.con_bandierine(None).bandierine_disponibili);
    }
}
