//! La chiave d'identità si legge dal portachiavi SOLO al momento della firma e non resta in memoria
//! (fondatore, 20/10/2026). Cosa si prova qui:
//!   - ogni firma legge il portachiavi (nessuna chiave tenuta da parte tra una firma e l'altra);
//!   - chi legge riceve il segreto in un `Zeroizing`, la chiave è `ZeroizeOnDrop` (controllo alla compilazione);
//!   - senza identità salvata non si firma.
//! Limite dichiarato: Rust non garantisce che uno spostamento non lasci copie sullo stack; si cancellano
//! tutte le copie che il nostro codice tiene.

use m4tr1x_identity::creazione::recupera;
use m4tr1x_identity::portachiavi::{firma_come_identita, Memoria, Portachiavi};
use m4tr1x_identity::{ChiaveRuolo, IdentityError};
use std::sync::atomic::{AtomicU32, Ordering};
use zeroize::{ZeroizeOnDrop, Zeroizing};

/// Portachiavi che conta le letture.
#[derive(Default)]
struct Contato {
    dentro: Memoria,
    letture: AtomicU32,
}

impl Portachiavi for Contato {
    fn scrivi(&self, voce: &str, segreto: &[u8; 32]) -> Result<(), IdentityError> {
        self.dentro.scrivi(voce, segreto)
    }
    fn leggi(&self, voce: &str) -> Result<Option<Zeroizing<[u8; 32]>>, IdentityError> {
        self.letture.fetch_add(1, Ordering::SeqCst);
        self.dentro.leggi(voce)
    }
    fn cancella(&self, voce: &str) -> Result<(), IdentityError> {
        self.dentro.cancella(voce)
    }
}

const PAROLE_PROVA: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";

#[test]
fn ogni_firma_legge_il_portachiavi_nessuna_chiave_tenuta() {
    let p = Contato::default();
    let id = recupera(PAROLE_PROVA, &p).unwrap();
    let prima = p.letture.load(Ordering::SeqCst);
    let d = [7u8; 32];
    let s1 = firma_come_identita(&p, &d).unwrap();
    let s2 = firma_come_identita(&p, &d).unwrap();
    assert_eq!(p.letture.load(Ordering::SeqCst) - prima, 2, "due firme = due letture: la chiave non resta tra una firma e l'altra");
    // firme valide della chiave d'identità salvata
    let pk = k256::schnorr::VerifyingKey::from_bytes(&hex(&id.identity)).unwrap();
    for s in [s1, s2] {
        pk.verify_raw(&d, &k256::schnorr::Signature::try_from(s.as_slice()).unwrap()).unwrap();
    }
}

#[test]
fn senza_identita_salvata_non_si_firma() {
    assert!(firma_come_identita(&Contato::default(), &[1u8; 32]).is_err());
}

#[test]
fn segreti_cancellati_per_costruzione() {
    fn zeroize_on_drop<T: ZeroizeOnDrop>() {}
    zeroize_on_drop::<ChiaveRuolo>();
    // il tipo restituito dal portachiavi è Zeroizing: lo impone il trait (questa riga non compilerebbe altrimenti)
    let _: fn(&Memoria, &str) -> Result<Option<Zeroizing<[u8; 32]>>, IdentityError> = <Memoria as Portachiavi>::leggi;
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}
