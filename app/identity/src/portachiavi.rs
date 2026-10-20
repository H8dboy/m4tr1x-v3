//! Portachiavi del sistema (KEYS.md: K1, K1d, K2 "Portachiavi del dispositivo").
//!
//! Si salvano SOLO i segreti dei ruoli identity, dm e wallet (32 byte ciascuno).
//! Mai il seme, mai le parole, mai la revoca (K1r si ricava dal seme quando serve).
//!
//! Compromesso dichiarato: su Windows è Gestione credenziali, protetta dall'accesso
//! dell'utente (DPAPI). Un programma che gira come lo stesso utente può leggerla:
//! protegge da copie del disco e da altri utenti del PC, non da malware già dentro.

use crate::{ChiaveRuolo, IdentityError, Ruolo};
use std::collections::HashMap;
use std::sync::Mutex;
use zeroize::Zeroizing;

const SERVIZIO: &str = "org.m4tr1x.app";

/// Ruoli che si possono salvare: la revoca non c'è, per costruzione.
pub const RUOLI_SALVATI: [Ruolo; 3] = [Ruolo::Identity, Ruolo::Dm, Ruolo::Wallet];

fn voce(ruolo: Ruolo, versione: u32) -> String {
    format!("{}-v{}", ruolo.nome(), versione)
}

pub trait Portachiavi: Send + Sync {
    fn scrivi(&self, voce: &str, segreto: &[u8; 32]) -> Result<(), IdentityError>;
    /// Il segreto torna in un `Zeroizing`: si cancella dalla memoria quando chi legge ha finito.
    fn leggi(&self, voce: &str) -> Result<Option<Zeroizing<[u8; 32]>>, IdentityError>;
    fn cancella(&self, voce: &str) -> Result<(), IdentityError>;
}

pub fn salva(p: &dyn Portachiavi, k: &ChiaveRuolo) -> Result<(), IdentityError> {
    if !RUOLI_SALVATI.contains(&k.ruolo()) {
        return Err(IdentityError::Derivazione); // revoca: mai nel portachiavi
    }
    p.scrivi(&voce(k.ruolo(), k.versione()), k.segreto_per_portachiavi())
}

pub fn carica(p: &dyn Portachiavi, ruolo: Ruolo, versione: u32) -> Result<Option<ChiaveRuolo>, IdentityError> {
    match p.leggi(&voce(ruolo, versione))? {
        // una sola copia del segreto, dentro ChiaveRuolo (ZeroizeOnDrop); s si cancella qui
        Some(s) => ChiaveRuolo::da_portachiavi(ruolo, versione, &s).map(Some),
        None => Ok(None),
    }
}

/// Firma `digest` con la chiave d'IDENTITÀ (versione 1), letta dal portachiavi SOLO per questa firma:
/// la chiave esiste in memoria per la durata della chiamata e si cancella all'uscita (ZeroizeOnDrop).
/// Chi firma post (il guscio) usa solo questa e non tiene mai una `ChiaveRuolo` (fondatore, 20/10/2026).
pub fn firma_come_identita(p: &dyn Portachiavi, digest: &[u8; 32]) -> Result<[u8; 64], IdentityError> {
    let k = carica(p, Ruolo::Identity, 1)?.ok_or(IdentityError::Portachiavi("nessuna identità salvata".into()))?;
    k.firma_schnorr(digest).ok_or(IdentityError::Derivazione)
    // `k` esce di scena qui: segreto cancellato
}

/// Gestione credenziali di Windows (e i portachiavi nativi degli altri sistemi).
pub struct Sistema;

impl Portachiavi for Sistema {
    fn scrivi(&self, voce: &str, segreto: &[u8; 32]) -> Result<(), IdentityError> {
        keyring::Entry::new(SERVIZIO, voce)
            .and_then(|e| e.set_secret(segreto))
            .map_err(|e| IdentityError::Portachiavi(e.to_string()))
    }
    fn leggi(&self, voce: &str) -> Result<Option<Zeroizing<[u8; 32]>>, IdentityError> {
        let e = keyring::Entry::new(SERVIZIO, voce).map_err(|e| IdentityError::Portachiavi(e.to_string()))?;
        match e.get_secret() {
            // il Vec del portachiavi si cancella prima di essere liberato (20/10/2026: prima restava nella memoria liberata)
            Ok(v) => {
                let v = Zeroizing::new(v);
                let mut s = Zeroizing::new([0u8; 32]);
                if v.len() != 32 {
                    return Err(IdentityError::Portachiavi("voce di lunghezza sbagliata".into()));
                }
                s.copy_from_slice(&v);
                Ok(Some(s))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(IdentityError::Portachiavi(e.to_string())),
        }
    }
    fn cancella(&self, voce: &str) -> Result<(), IdentityError> {
        let e = keyring::Entry::new(SERVIZIO, voce).map_err(|e| IdentityError::Portachiavi(e.to_string()))?;
        match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(IdentityError::Portachiavi(e.to_string())),
        }
    }
}

/// Portachiavi in memoria, per i test: permette di controllare cosa viene salvato.
#[derive(Default)]
pub struct Memoria {
    pub voci: Mutex<HashMap<String, [u8; 32]>>,
}

impl Portachiavi for Memoria {
    fn scrivi(&self, voce: &str, segreto: &[u8; 32]) -> Result<(), IdentityError> {
        self.voci.lock().unwrap().insert(voce.to_string(), *segreto);
        Ok(())
    }
    fn leggi(&self, voce: &str) -> Result<Option<Zeroizing<[u8; 32]>>, IdentityError> {
        Ok(self.voci.lock().unwrap().get(voce).map(|s| Zeroizing::new(*s)))
    }
    fn cancella(&self, voce: &str) -> Result<(), IdentityError> {
        self.voci.lock().unwrap().remove(voce);
        Ok(())
    }
}
