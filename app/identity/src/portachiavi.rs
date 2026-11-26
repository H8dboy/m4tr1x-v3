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

// ─── Rotazione della chiave DM (K1d, KEYS.md; fondatore 24/11/2026: «rotazione falla pure») ─────────────────
//
// `dm-v1` nasce dal seme (si ritrova con le 24 parole) e resta sempre. Le versioni dopo (`dm-v2`, `dm-v3`, …)
// sono CASUALI: il seme non si salva mai, quindi non si possono ricavare da lì, e una chiave rubata espone solo i
// messaggi del suo periodo. Si tengono la corrente e le `DM_TENUTE - 1` precedenti (per leggere i messaggi
// recenti e i messaggi che arrivano ancora alla chiave vecchia); quelle più vecchie si cancellano.
// Quale versione è la corrente sta nella voce `dm-corrente` (il numero nei primi 4 byte, il resto zero): non è
// un segreto, ma il portachiavi è l'unico posto che l'app ha e che segue l'identità.

/// Voce del portachiavi con il numero della versione DM corrente.
pub const VOCE_DM_CORRENTE: &str = "dm-corrente";
/// Chiavi DM ruotate tenute: la corrente e le precedenti fino a questo numero (oltre a `dm-v1`, che resta).
pub const DM_TENUTE: u32 = 3;

/// Versione della chiave DM corrente (1 se non si è mai ruotato).
pub fn versione_dm_corrente(p: &dyn Portachiavi) -> Result<u32, IdentityError> {
    Ok(match p.leggi(VOCE_DM_CORRENTE)? {
        Some(b) => {
            let v = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            if v < 1 || b[4..].iter().any(|&x| x != 0) {
                return Err(IdentityError::Portachiavi("voce dm-corrente non valida".into()));
            }
            v
        }
        None => 1,
    })
}

/// Le chiavi DM da usare, la corrente PER PRIMA: poi le ruotate ancora tenute, poi `dm-v1`. Quelle che mancano
/// si saltano; se manca la corrente è un errore (non si cifra mai per una chiave vecchia credendola nuova).
pub fn chiavi_dm(p: &dyn Portachiavi) -> Result<Vec<ChiaveRuolo>, IdentityError> {
    let n = versione_dm_corrente(p)?;
    let corrente = carica(p, Ruolo::Dm, n)?.ok_or(IdentityError::Portachiavi("manca la chiave DM corrente".into()))?;
    let mut tutte = vec![corrente];
    let piu_vecchia = n.saturating_sub(DM_TENUTE - 1).max(2);
    for v in (piu_vecchia..n).rev() {
        if let Some(k) = carica(p, Ruolo::Dm, v)? {
            tutte.push(k);
        }
    }
    if n != 1 {
        if let Some(k) = carica(p, Ruolo::Dm, 1)? {
            tutte.push(k);
        }
    }
    Ok(tutte)
}

/// Solo la chiave DM corrente (per incartare un messaggio).
pub fn chiave_dm_corrente(p: &dyn Portachiavi) -> Result<ChiaveRuolo, IdentityError> {
    let n = versione_dm_corrente(p)?;
    carica(p, Ruolo::Dm, n)?.ok_or(IdentityError::Portachiavi("manca la chiave DM corrente".into()))
}

/// Le chiavi DM PUBBLICHE (la corrente per prima): i segreti si cancellano prima di uscire.
pub fn dm_pubbliche(p: &dyn Portachiavi) -> Result<Vec<String>, IdentityError> {
    Ok(chiavi_dm(p)?.iter().map(ChiaveRuolo::pubblica_hex).collect())
}

/// Ruota la chiave DM: una chiave nuova casuale diventa la corrente, quella uscita dalla finestra delle
/// `DM_TENUTE` si cancella. Ritorna la nuova chiave PUBBLICA. Ordine pensato per un'interruzione a metà: prima si
/// scrive la chiave nuova, poi il numero della corrente, poi si cancella la vecchia; in nessun momento manca la
/// chiave corrente.
pub fn ruota_dm(p: &dyn Portachiavi) -> Result<String, IdentityError> {
    let n = versione_dm_corrente(p)?;
    let nuova_v = n.checked_add(1).ok_or(IdentityError::Derivazione)?;
    let nuova = loop {
        let mut s = Zeroizing::new([0u8; 32]);
        getrandom::fill(&mut s[..]).map_err(|_| IdentityError::Casuale)?;
        // fuori dall'intervallo valido di secp256k1 (probabilità ~2^-128): se ne prende un'altra
        if let Ok(k) = ChiaveRuolo::da_portachiavi(Ruolo::Dm, nuova_v, &s) {
            break k;
        }
    };
    salva(p, &nuova)?;
    let mut numero = [0u8; 32];
    numero[..4].copy_from_slice(&nuova_v.to_le_bytes());
    p.scrivi(VOCE_DM_CORRENTE, &numero)?;
    let uscita = nuova_v.saturating_sub(DM_TENUTE);
    if uscita >= 2 {
        p.cancella(&voce(Ruolo::Dm, uscita))?;
    }
    Ok(nuova.pubblica_hex())
}

/// Cancella le chiavi DM ruotate e il numero della corrente (resta `dm-v1`). Si usa quando nel portachiavi entra
/// un'identità DIVERSA: le chiavi ruotate della vecchia non devono diventare le sue.
pub fn dimentica_dm_ruotate(p: &dyn Portachiavi) -> Result<(), IdentityError> {
    let n = versione_dm_corrente(p).unwrap_or(1);
    for v in n.saturating_sub(DM_TENUTE - 1).max(2)..=n {
        p.cancella(&voce(Ruolo::Dm, v))?;
    }
    p.cancella(VOCE_DM_CORRENTE)
}

/// Cancella dal portachiavi TUTTE le chiavi di questa identità (account cancellato, I-68): le chiavi DM ruotate e
/// il numero della corrente, poi identity, dm e wallet. Dopo, `identita_salvata` dice None. Chi ha le 24 parole può
/// ancora ricostruirle: il seme non è mai stato qui.
pub fn dimentica_tutto(p: &dyn Portachiavi) -> Result<(), IdentityError> {
    dimentica_dm_ruotate(p)?;
    for r in RUOLI_SALVATI {
        p.cancella(&voce(r, 1))?;
    }
    Ok(())
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
