//! Messaggi privati (F5, ADR-0018): NIP-17 con chiave DM separata. SCHELETRO: le prove (tests/) sono scritte
//! prima del codice e falliscono finché le funzioni non sono implementate.

use m4tr1x_identity::ChiaveRuolo;
use m4tr1x_protocol::{Evento, Rete};
use serde_json::Value;

pub mod nip44;

#[cfg(feature = "prova")]
pub mod prova;

/// Kind dell'annuncio della chiave DM (NIP-17: "relay preferiti per i DM"), sostituibile, firmato dall'identità.
pub const KIND_ANNUNCIO_DM: u16 = 10477;
/// Kind della busta (NIP-59): l'UNICO kind con cui un DM esce dall'app (I-19).
pub const KIND_BUSTA: u16 = 1059;
/// La data della busta è sfalsata indietro fino a 2 giorni (I-20).
pub const SFASAMENTO_MASSIMO: u64 = 2 * 24 * 3600;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ErroreDm {
    #[error("non ancora implementato")]
    NonImplementato,
    #[error("chiave non valida")]
    Chiave,
    #[error("messaggio non valido: {0}")]
    Messaggio(&'static str),
    #[error("cifratura: {0}")]
    Cifratura(&'static str),
    #[error("busta scartata: {0}")]
    Scartata(&'static str),
    #[error("evento rifiutato dal protocollo: {0}")]
    Protocollo(String),
}

/// Fonte di casualità: il sistema in produzione, una finta nelle prove.
pub trait Entropia {
    fn riempi(&mut self, dove: &mut [u8]);
}

pub struct EntropiaSistema;
impl Entropia for EntropiaSistema {
    fn riempi(&mut self, _dove: &mut [u8]) {
        unimplemented!("F5: getrandom")
    }
}

/// Le due buste di un messaggio: una per il destinatario, una per il mittente (la sua copia).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Buste {
    pub per_destinatario: Evento,
    pub per_mittente: Evento,
}

/// Un messaggio aperto e controllato (forma, firma del sigillo, coerenza sigillo/messaggio).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aperto {
    /// Chiave DM che ha sigillato il messaggio: NON è ancora un'identità (vedi [`identita_del_mittente`]).
    pub dm_mittente: String,
    pub testo: String,
    /// Ora dichiarata nel messaggio.
    pub created_at: u64,
    /// Ora (sfalsata) del sigillo, per le prove.
    pub sigillo_created_at: u64,
}

/// Costruisce le due buste (kind 1059, firmate da chiavi usa e getta, data sfalsata) di un messaggio da
/// `mittente` (ruolo Dm) alla chiave DM `dm_destinatario`.
pub fn incarta(
    _rete: Rete,
    _mittente: &ChiaveRuolo,
    _dm_destinatario: &str,
    _testo: &str,
    _ora: u64,
    _entropia: &mut dyn Entropia,
) -> Result<Buste, ErroreDm> {
    Err(ErroreDm::NonImplementato)
}

/// Apre una busta ricevuta con la propria chiave DM e controlla tutto (I-61).
pub fn apri(_rete: Rete, _mia: &ChiaveRuolo, _busta: &Value) -> Result<Aperto, ErroreDm> {
    Err(ErroreDm::NonImplementato)
}

/// Annuncio firmato dall'identità: le chiavi DM attive, ognuna con la prova di possesso firmata dalla chiave
/// DM stessa (I-60), e i relay per i DM.
pub fn annuncio_dm(
    _rete: Rete,
    _identita: &ChiaveRuolo,
    _dm_attive: &[&ChiaveRuolo],
    _relay: &[String],
    _ora: u64,
) -> Result<Evento, ErroreDm> {
    Err(ErroreDm::NonImplementato)
}

/// Le chiavi DM che un annuncio dichiara valide per la sua identità (vuoto se manca una prova di possesso).
pub fn dm_dichiarate(_rete: Rete, _annuncio: &Value) -> Result<(String, Vec<String>), ErroreDm> {
    Err(ErroreDm::NonImplementato)
}

/// L'identità a cui attribuire un messaggio sigillato da `dm_mittente`: solo se l'annuncio PIÙ RECENTE di
/// un'identità elenca quella chiave con la prova di possesso. Altrimenti `None` (I-60).
pub fn identita_del_mittente(_rete: Rete, _annunci: &[Value], _dm_mittente: &str) -> Option<String> {
    None
}
