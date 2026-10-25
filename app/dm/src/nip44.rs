//! NIP-44 v2: chiave di conversazione, chiavi del messaggio, padding, cifratura. SCHELETRO.

use crate::ErroreDm;

pub struct ChiaviMessaggio {
    pub chacha_key: [u8; 32],
    pub chacha_nonce: [u8; 12],
    pub hmac_key: [u8; 32],
}

/// Chiave di conversazione da un segreto e una pubblica x-only (solo vettori e prove: nell'app il segreto DM
/// non esce dal crate dell'identità).
pub fn chiave_conversazione(_segreto: &[u8; 32], _pubblica: &[u8; 32]) -> Result<[u8; 32], ErroreDm> {
    Err(ErroreDm::NonImplementato)
}

pub fn chiavi_messaggio(_conversazione: &[u8; 32], _nonce: &[u8; 32]) -> Result<ChiaviMessaggio, ErroreDm> {
    Err(ErroreDm::NonImplementato)
}

pub fn lunghezza_con_padding(_n: usize) -> usize {
    0
}

pub fn cifra_con_nonce(_conversazione: &[u8; 32], _nonce: &[u8; 32], _testo: &str) -> Result<String, ErroreDm> {
    Err(ErroreDm::NonImplementato)
}

pub fn decifra(_conversazione: &[u8; 32], _payload: &str) -> Result<String, ErroreDm> {
    Err(ErroreDm::NonImplementato)
}
