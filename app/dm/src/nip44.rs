//! NIP-44 v2: chiave di conversazione, chiavi del messaggio, padding, cifratura ChaCha20 + HMAC-SHA256.
//! Provato con i vettori UFFICIALI (tests/vettori/nip44.vectors.json). Il segreto DM dell'utente NON passa di
//! qui: arriva soltanto il segreto condiviso già calcolato dal crate dell'identità (`ecdh_dm`).

use crate::ErroreDm;
use base64::{engine::general_purpose::STANDARD, Engine};
use chacha20::{
    cipher::{KeyIvInit, StreamCipher},
    ChaCha20,
};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

const SALE: &[u8] = b"nip44-v2";
const VERSIONE: u8 = 2;
const MASSIMO_TESTO: usize = 65535;

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct ChiaviMessaggio {
    pub chacha_key: [u8; 32],
    pub chacha_nonce: [u8; 12],
    pub hmac_key: [u8; 32],
}

/// Chiave di conversazione dal segreto condiviso ECDH (coordinata x).
pub fn chiave_da_condiviso(x: &[u8; 32]) -> [u8; 32] {
    let (prk, _) = Hkdf::<Sha256>::extract(Some(SALE), x);
    let mut k = [0u8; 32];
    k.copy_from_slice(&prk);
    k
}

/// Chiave di conversazione da un segreto e una pubblica x-only. Per i vettori e per le chiavi USA E GETTA delle
/// buste: il segreto della chiave DM dell'utente non arriva mai qui (si usa `ChiaveRuolo::ecdh_dm`).
pub fn chiave_conversazione(segreto: &[u8; 32], pubblica: &[u8; 32]) -> Result<[u8; 32], ErroreDm> {
    let sk = k256::SecretKey::from_slice(segreto).map_err(|_| ErroreDm::Chiave)?;
    let mut sec1 = [0x02u8; 33];
    sec1[1..].copy_from_slice(pubblica);
    let pk = k256::PublicKey::from_sec1_bytes(&sec1).map_err(|_| ErroreDm::Chiave)?;
    let mut x = [0u8; 32];
    x.copy_from_slice(k256::ecdh::diffie_hellman(sk.to_nonzero_scalar(), pk.as_affine()).raw_secret_bytes());
    let k = chiave_da_condiviso(&x);
    x.zeroize();
    Ok(k)
}

pub fn chiavi_messaggio(conversazione: &[u8; 32], nonce: &[u8; 32]) -> Result<ChiaviMessaggio, ErroreDm> {
    let hk = Hkdf::<Sha256>::from_prk(conversazione).map_err(|_| ErroreDm::Cifratura("chiave di conversazione"))?;
    let mut out = [0u8; 76];
    hk.expand(nonce, &mut out).map_err(|_| ErroreDm::Cifratura("derivazione"))?;
    let mut m = ChiaviMessaggio { chacha_key: [0; 32], chacha_nonce: [0; 12], hmac_key: [0; 32] };
    m.chacha_key.copy_from_slice(&out[..32]);
    m.chacha_nonce.copy_from_slice(&out[32..44]);
    m.hmac_key.copy_from_slice(&out[44..]);
    out.zeroize();
    Ok(m)
}

/// Lunghezza del testo dopo il riempimento (scaglioni: 32, poi multipli crescenti).
pub fn lunghezza_con_padding(n: usize) -> usize {
    if n <= 32 {
        return 32;
    }
    let successiva = 1usize << (usize::BITS - (n - 1).leading_zeros());
    let blocco = if successiva <= 256 { 32 } else { successiva / 8 };
    blocco * ((n - 1) / blocco + 1)
}

fn riempi(testo: &str) -> Result<Vec<u8>, ErroreDm> {
    let b = testo.as_bytes();
    if b.is_empty() || b.len() > MASSIMO_TESTO {
        return Err(ErroreDm::Messaggio("il testo deve avere da 1 a 65535 byte"));
    }
    let mut v = Vec::with_capacity(2 + lunghezza_con_padding(b.len()));
    v.extend_from_slice(&(b.len() as u16).to_be_bytes());
    v.extend_from_slice(b);
    v.resize(2 + lunghezza_con_padding(b.len()), 0);
    Ok(v)
}

fn mac(chiave: &[u8; 32], nonce: &[u8; 32], cifrato: &[u8]) -> Hmac<Sha256> {
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(chiave).expect("HMAC accetta ogni lunghezza di chiave");
    m.update(nonce);
    m.update(cifrato);
    m
}

pub fn cifra_con_nonce(conversazione: &[u8; 32], nonce: &[u8; 32], testo: &str) -> Result<String, ErroreDm> {
    let k = chiavi_messaggio(conversazione, nonce)?;
    let mut dati = riempi(testo)?;
    ChaCha20::new((&k.chacha_key).into(), (&k.chacha_nonce).into()).apply_keystream(&mut dati);
    let tag = mac(&k.hmac_key, nonce, &dati).finalize().into_bytes();
    let mut fuori = Vec::with_capacity(1 + 32 + dati.len() + 32);
    fuori.push(VERSIONE);
    fuori.extend_from_slice(nonce);
    fuori.extend_from_slice(&dati);
    fuori.extend_from_slice(&tag);
    Ok(STANDARD.encode(fuori))
}

pub fn decifra(conversazione: &[u8; 32], payload: &str) -> Result<String, ErroreDm> {
    let no = |m: &'static str| ErroreDm::Cifratura(m);
    if payload.starts_with('#') {
        return Err(no("formato non supportato"));
    }
    if !(132..=87472).contains(&payload.len()) {
        return Err(no("lunghezza del payload"));
    }
    let dati = STANDARD.decode(payload).map_err(|_| no("base64"))?;
    if !(99..=65603).contains(&dati.len()) {
        return Err(no("lunghezza dei dati"));
    }
    if dati[0] != VERSIONE {
        return Err(no("versione sconosciuta"));
    }
    let nonce: [u8; 32] = dati[1..33].try_into().expect("lunghezza controllata");
    let (cifrato, tag) = dati[33..].split_at(dati.len() - 33 - 32);
    let k = chiavi_messaggio(conversazione, &nonce)?;
    mac(&k.hmac_key, &nonce, cifrato).verify_slice(tag).map_err(|_| no("MAC non valido"))?;
    let mut chiaro = cifrato.to_vec();
    ChaCha20::new((&k.chacha_key).into(), (&k.chacha_nonce).into()).apply_keystream(&mut chiaro);
    let n = u16::from_be_bytes([chiaro[0], chiaro[1]]) as usize;
    if n == 0 || 2 + n > chiaro.len() || chiaro.len() != 2 + lunghezza_con_padding(n) {
        chiaro.zeroize();
        return Err(no("riempimento non valido"));
    }
    let testo = String::from_utf8(chiaro[2..2 + n].to_vec()).map_err(|_| no("testo non UTF-8"));
    chiaro.zeroize();
    testo
}
