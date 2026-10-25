//! SOLO PER LE PROVE (feature `prova`, mai nell'app): buste volutamente difettose per provare che
//! `apri` le scarta (I-61). SCHELETRO.

use crate::{ErroreDm, Entropia};
use m4tr1x_identity::ChiaveRuolo;
use m4tr1x_protocol::Rete;
use serde_json::Value;

/// Cosa c'è di sbagliato nella busta forgiata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Difetto {
    /// il sigillo è firmato da una chiave diversa da quella del messaggio (`pubkey` del messaggio ≠ sigillo)
    SigilloDiUnAltro,
    /// la firma del sigillo è alterata
    FirmaDelSigilloFalsa,
    /// il messaggio interno ha una firma (NIP-17: non deve averla)
    MessaggioFirmato,
    /// il messaggio interno non è kind 14
    KindDelMessaggioSbagliato,
    /// il sigillo non è kind 13
    KindDelSigilloSbagliato,
    /// versione NIP-44 sconosciuta nel contenuto della busta
    VersioneSconosciuta,
    /// il MAC della busta è alterato
    MacAlterato,
    /// il messaggio interno non porta il destinatario giusto (tag p) per chi apre
    DestinatarioSbagliato,
    /// l'ora del messaggio nel futuro lontano
    OraNelFuturo,
}

/// Una busta kind 1059 per `destinatario_dm` con il difetto richiesto.
pub fn incarta_difettosa(
    _difetto: Difetto,
    _rete: Rete,
    _mittente: &ChiaveRuolo,
    _destinatario_dm: &str,
    _testo: &str,
    _ora: u64,
    _entropia: &mut dyn Entropia,
) -> Result<Value, ErroreDm> {
    Err(ErroreDm::NonImplementato)
}
