//! SOLO PER LE PROVE (feature `prova`, mai nell'app): buste volutamente difettose per provare che
//! `apri` le scarta (I-61). Usa lo stesso codice di `incarta`, con UN difetto in più.

use crate::{costruisci, Entropia, ErroreDm, Guasto};
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
    /// l'id del messaggio interno non è quello che il contenuto produce
    IdDelMessaggioSbagliato,
    /// l'id del sigillo non è quello che il contenuto produce (firmato lo stesso: la firma è valida)
    IdDelSigilloSbagliato,
    /// l'ora del sigillo nel futuro lontano
    SigilloNelFuturo,
    /// il tag p della busta indica un altro (la busta è cifrata per chi la apre)
    TagPDellaBustaSbagliato,
}

impl From<Difetto> for Guasto {
    fn from(d: Difetto) -> Self {
        match d {
            Difetto::SigilloDiUnAltro => Guasto::SigilloDiUnAltro,
            Difetto::FirmaDelSigilloFalsa => Guasto::FirmaDelSigilloFalsa,
            Difetto::MessaggioFirmato => Guasto::MessaggioFirmato,
            Difetto::KindDelMessaggioSbagliato => Guasto::KindDelMessaggioSbagliato,
            Difetto::KindDelSigilloSbagliato => Guasto::KindDelSigilloSbagliato,
            Difetto::VersioneSconosciuta => Guasto::VersioneSconosciuta,
            Difetto::MacAlterato => Guasto::MacAlterato,
            Difetto::DestinatarioSbagliato => Guasto::DestinatarioSbagliato,
            Difetto::OraNelFuturo => Guasto::OraNelFuturo,
            Difetto::IdDelMessaggioSbagliato => Guasto::IdDelMessaggioSbagliato,
            Difetto::IdDelSigilloSbagliato => Guasto::IdDelSigilloSbagliato,
            Difetto::SigilloNelFuturo => Guasto::SigilloNelFuturo,
            Difetto::TagPDellaBustaSbagliato => Guasto::TagPDellaBustaSbagliato,
        }
    }
}

/// Una busta kind 1059 per `destinatario_dm` con il difetto richiesto.
pub fn incarta_difettosa(
    difetto: Difetto,
    rete: Rete,
    mittente: &ChiaveRuolo,
    destinatario_dm: &str,
    testo: &str,
    ora: u64,
    entropia: &mut dyn Entropia,
) -> Result<Value, ErroreDm> {
    let (per_destinatario, _) = costruisci(Some(difetto.into()), rete, mittente, destinatario_dm, testo, ora, entropia)?;
    serde_json::to_value(&per_destinatario).map_err(|_| ErroreDm::Messaggio("serializzazione"))
}
