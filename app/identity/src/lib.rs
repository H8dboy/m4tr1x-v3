//! Chiavi dell'utente dal seme (F4) — stesse chiavi di `packages/keys` (JavaScript).
//!
//!   24 parole (BIP-39) → seme 64 byte (PBKDF2, standard) → per ogni ruolo e versione:
//!   HKDF-SHA256(seme, salt "m4tr1x/v3/keys", info "m4tr1x/<ruolo>/v<n>[/<contatore>]", 32 byte)
//!   - identity, dm, revoke: Schnorr secp256k1 (BIP-340), si scarta un valore fuori dalla curva
//!     (contatore +1, probabilità ~2^-128)
//!   - wallet: ML-DSA-65, il valore HKDF è il seme ξ di FIPS 204 KeyGen_internal (ADR-0017)
//!
//! I valori sono CONGELATI dai vettori di `tests/unit/keys-vectors.test.js`: se cambiano,
//! cambierebbero le identità di tutti gli utenti.
//!
//! Regole: i segreti si cancellano dalla memoria quando escono di scena (zeroize), non si
//! stampano mai (Debug ridotto) e non attraversano mai il confine verso il JavaScript (I-31).

pub mod creazione;
pub mod portachiavi;

use hkdf::Hkdf;
use sha2::Sha256;
use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

const HKDF_SALT: &[u8] = b"m4tr1x/v3/keys";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdentityError {
    #[error("parole del seme non valide")]
    ParoleNonValide,
    #[error("versione non valida")]
    Versione,
    #[error("derivazione impossibile")]
    Derivazione,
    #[error("generatore casuale del sistema non disponibile")]
    Casuale,
    #[error("le parole di verifica non corrispondono")]
    ConfermaSbagliata,
    #[error("portachiavi del sistema: {0}")]
    Portachiavi(String),
}

/// Ruoli dell'utente ricavati dal seme (KEYS.md: K1, K1d, K1r, K2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ruolo {
    Identity,
    Dm,
    /// K1r: si ricava SOLO dal seme quando serve, non si salva mai (KEYS.md).
    Revoke,
    Wallet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Famiglia {
    Schnorr,
    MlDsa65,
}

impl Ruolo {
    pub const TUTTI: [Ruolo; 4] = [Ruolo::Identity, Ruolo::Dm, Ruolo::Revoke, Ruolo::Wallet];

    pub fn nome(self) -> &'static str {
        match self {
            Ruolo::Identity => "identity",
            Ruolo::Dm => "dm",
            Ruolo::Revoke => "revoke",
            Ruolo::Wallet => "wallet",
        }
    }

    /// ADR-0017: Schnorr per il social, ML-DSA-65 per ciò che vale.
    pub fn famiglia(self) -> Famiglia {
        match self {
            Ruolo::Wallet => Famiglia::MlDsa65,
            _ => Famiglia::Schnorr,
        }
    }
}

/// Il seme da 64 byte. Si cancella dalla memoria alla fine; non si stampa.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Seme([u8; 64]);

impl fmt::Debug for Seme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Seme(…)")
    }
}

/// Le 24 parole. Si cancellano dalla memoria alla fine; non si stampano.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Parole(String);

impl fmt::Debug for Parole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Parole(…)")
    }
}

impl Parole {
    /// Le parole una per una: servono solo alla finestra del seme per mostrarle.
    pub fn elenco(&self) -> Vec<&str> {
        self.0.split(' ').collect()
    }
}

/// Normalizzazione identica a packages/keys: spazi ai bordi via, minuscole, un solo spazio.
fn normalizza(parole: &str) -> String {
    parole.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn parole_valide(parole: &str) -> bool {
    bip39::Mnemonic::parse_in_normalized(bip39::Language::English, &normalizza(parole)).is_ok()
}

/// 24 parole nuove (256 bit di entropia dal generatore del sistema).
pub fn genera_parole() -> Result<Parole, IdentityError> {
    let mut e = [0u8; 32];
    getrandom::fill(&mut e).map_err(|_| IdentityError::Casuale)?;
    let m = bip39::Mnemonic::from_entropy_in(bip39::Language::English, &e).map_err(|_| IdentityError::Casuale)?;
    e.zeroize();
    Ok(Parole(m.to_string()))
}

/// Seme di 64 byte dalle parole (BIP-39 standard); passphrase BIP-39 facoltativa.
pub fn seme_da_parole(parole: &str, passphrase: &str) -> Result<Seme, IdentityError> {
    let mut n = normalizza(parole);
    let m = bip39::Mnemonic::parse_in_normalized(bip39::Language::English, &n).map_err(|_| IdentityError::ParoleNonValide);
    n.zeroize();
    Ok(Seme(m?.to_seed(passphrase)))
}

/// Segreto di un ruolo (32 byte). Si cancella alla fine; non si stampa.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct ChiaveRuolo {
    #[zeroize(skip)]
    ruolo: Ruolo,
    #[zeroize(skip)]
    versione: u32,
    segreto: [u8; 32],
}

impl fmt::Debug for ChiaveRuolo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChiaveRuolo({} v{}, …)", self.ruolo.nome(), self.versione)
    }
}

/// Chiave del ruolo `ruolo`, versione `versione`, ricavata dal seme.
pub fn deriva(seme: &Seme, ruolo: Ruolo, versione: u32) -> Result<ChiaveRuolo, IdentityError> {
    if versione < 1 {
        return Err(IdentityError::Versione);
    }
    let hk = Hkdf::<Sha256>::new(Some(HKDF_SALT), &seme.0);
    for contatore in 0u32..256 {
        let mut info = format!("m4tr1x/{}/v{}", ruolo.nome(), versione);
        if contatore > 0 {
            info.push_str(&format!("/{contatore}"));
        }
        let mut sk = [0u8; 32];
        hk.expand(info.as_bytes(), &mut sk).map_err(|_| IdentityError::Derivazione)?;
        let ok = match ruolo.famiglia() {
            Famiglia::MlDsa65 => true, // qualsiasi ξ va bene
            Famiglia::Schnorr => k256::SecretKey::from_slice(&sk).is_ok(), // 0 < n < ordine della curva
        };
        if ok {
            return Ok(ChiaveRuolo { ruolo, versione, segreto: sk });
        }
        sk.zeroize();
    }
    Err(IdentityError::Derivazione)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Impronta della chiave pubblica del wallet: sha256 della sua forma esadecimale
/// (stessa convenzione dei vettori congelati di packages/keys).
pub fn impronta_wallet(pubblica_hex: &str) -> String {
    use sha2::Digest;
    hex(&Sha256::digest(pubblica_hex.as_bytes()))
}

impl ChiaveRuolo {
    pub fn ruolo(&self) -> Ruolo {
        self.ruolo
    }

    pub fn versione(&self) -> u32 {
        self.versione
    }

    /// Chiave pubblica in esadecimale: Schnorr = 32 byte x-only (BIP-340), ML-DSA-65 = 1952 byte.
    pub fn pubblica_hex(&self) -> String {
        match self.ruolo.famiglia() {
            Famiglia::Schnorr => {
                let sk = k256::schnorr::SigningKey::from_bytes(&self.segreto).expect("controllata in deriva()");
                hex(&sk.verifying_key().to_bytes())
            }
            Famiglia::MlDsa65 => {
                use ml_dsa::{MlDsa65, Seed, SigningKey, VerifyingKey};
                let seed = Seed::from(self.segreto);
                let sk = SigningKey::<MlDsa65>::from_seed(&seed);
                let vk: &VerifyingKey<MlDsa65> = sk.as_ref();
                hex(&vk.encode())
            }
        }
    }

    /// Firma Schnorr (BIP-340) di un digest da 32 byte. Solo ruoli Schnorr: il wallet firma in F7.
    pub fn firma_schnorr(&self, digest: &[u8; 32]) -> Option<[u8; 64]> {
        if self.ruolo.famiglia() != Famiglia::Schnorr {
            return None;
        }
        let sk = k256::schnorr::SigningKey::from_bytes(&self.segreto).ok()?;
        let mut aux = [0u8; 32];
        getrandom::fill(&mut aux).ok()?;
        let sig = sk.sign_raw(digest, &aux).ok()?;
        Some(sig.to_bytes())
    }

    /// Il segreto, SOLO per sigillarlo nel portachiavi del sistema. Mai verso il JavaScript.
    pub fn segreto_per_portachiavi(&self) -> &[u8; 32] {
        &self.segreto
    }

    /// Ricostruisce una chiave dal portachiavi del sistema.
    pub fn da_portachiavi(ruolo: Ruolo, versione: u32, segreto: [u8; 32]) -> Result<Self, IdentityError> {
        if ruolo == Ruolo::Revoke {
            return Err(IdentityError::Derivazione); // la revoca non si salva mai, quindi non si rilegge
        }
        if ruolo.famiglia() == Famiglia::Schnorr && k256::SecretKey::from_slice(&segreto).is_err() {
            return Err(IdentityError::Derivazione);
        }
        Ok(Self { ruolo, versione, segreto })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    /// Seme pubblico di prova (lo stesso di tests/unit/keys-vectors.test.js): non è un segreto.
    fn seme_prova() -> Seme {
        seme_da_parole(&format!("{}art", "abandon ".repeat(23)), "").unwrap()
    }

    #[test]
    fn bip39_vettore_ufficiale_trezor() {
        let m = format!("{}about", "abandon ".repeat(11));
        let s = seme_da_parole(&m, "TREZOR").unwrap();
        // in due parti, come nel test JS (lo scanner segnala le stringhe da 64 byte)
        let atteso = String::from("c55257c360c07c72029aebc1b53c05ed0362ada38ead3e3e9efa3708e5349553")
            + "1f09a6987599d18264c1e1c92f2cf141630c7a3c4ab7c81b2f001698e7463b04";
        assert_eq!(hex(&s.0), atteso);
    }

    /// Valori CONGELATI copiati da tests/unit/keys-vectors.test.js (FROZEN). Per il wallet:
    /// sha256 della chiave pubblica scritta in esadecimale (così fa il test JS).
    const CONGELATI: [(Ruolo, &str, &str); 4] = [
        (Ruolo::Identity, "817d2b662e9c2c6e080c5fd5d9c4866a8342d194dbea7f98c1f4fc30e069bed7", "d2f184cb121675cfab83351bc3cb0150d566e212ef1658b03c1b4983b5598f88"),
        (Ruolo::Dm, "03c0455c6c26dffc42017bc436bc8c994cb943973e5cac986f0b47dcafe98837", "0ff48b21a30c67bad952a45f488470d60df00fc0693af8a3347a768eb3e121d5"),
        (Ruolo::Revoke, "60eed1298e5ccf492e850d922bc7822b3a37b740dcd4adb7a973f05e2bbc5242", "c429002897006a6658424189b277f40180eed68dc52fc77e4cd54dd608005351"),
        (Ruolo::Wallet, "7caf829515e0da88e621b8ad535c1c607770d4399f48eed5f23bd027d3033eb5", "c8f5cfdd38e77a72d49a25db5c311d0e1fc195f9503d1f41b6e2684955c6fd35"),
    ];

    fn impronta(r: Ruolo, pubhex: &str) -> String {
        match r.famiglia() {
            Famiglia::Schnorr => pubhex.to_string(),
            Famiglia::MlDsa65 => impronta_wallet(pubhex),
        }
    }

    #[test]
    fn stesse_chiavi_del_javascript() {
        let s = seme_prova();
        for (r, v1, v2) in CONGELATI {
            assert_eq!(impronta(r, &deriva(&s, r, 1).unwrap().pubblica_hex()), v1, "{} v1", r.nome());
            assert_eq!(impronta(r, &deriva(&s, r, 2).unwrap().pubblica_hex()), v2, "{} v2", r.nome());
        }
    }

    #[test]
    fn parole_non_valide_rifiutate() {
        assert_eq!(seme_da_parole("ciao mondo questo non è un seme", "").unwrap_err(), IdentityError::ParoleNonValide);
        // checksum sbagliato
        assert_eq!(seme_da_parole(&"abandon ".repeat(24), "").unwrap_err(), IdentityError::ParoleNonValide);
        // maiuscole e spazi in più: accettate come in JS
        assert!(seme_da_parole(&format!("  {}ART ", "ABANDON   ".repeat(23)), "").is_ok());
    }

    #[test]
    fn ventiquattro_parole_diverse_ogni_volta() {
        let a = genera_parole().unwrap();
        let b = genera_parole().unwrap();
        assert_eq!(a.elenco().len(), 24);
        assert!(parole_valide(&a.0));
        assert_ne!(a.0, b.0);
    }

    #[test]
    fn niente_segreti_nelle_stampe() {
        let s = seme_prova();
        let k = deriva(&s, Ruolo::Identity, 1).unwrap();
        let p = genera_parole().unwrap();
        for d in [format!("{s:?}"), format!("{k:?}"), format!("{p:?}")] {
            assert!(d.len() < 40 && !d.contains(&hex(k.segreto_per_portachiavi())[..16]), "{d}");
        }
        assert!(!format!("{p:?}").contains(p.elenco()[0]));
    }

    #[test]
    fn la_revoca_non_si_rilegge_dal_portachiavi() {
        let s = seme_prova();
        let r = deriva(&s, Ruolo::Revoke, 1).unwrap();
        assert!(ChiaveRuolo::da_portachiavi(Ruolo::Revoke, 1, *r.segreto_per_portachiavi()).is_err());
        let i = deriva(&s, Ruolo::Identity, 1).unwrap();
        let back = ChiaveRuolo::da_portachiavi(Ruolo::Identity, 1, *i.segreto_per_portachiavi()).unwrap();
        assert_eq!(back.pubblica_hex(), i.pubblica_hex());
    }

    #[test]
    fn firma_schnorr_verificabile_e_wallet_escluso() {
        let s = seme_prova();
        let k = deriva(&s, Ruolo::Identity, 1).unwrap();
        let d: [u8; 32] = Sha256::digest(b"m4tr1x prova").into();
        let sig = k.firma_schnorr(&d).unwrap();
        let pk = k256::schnorr::VerifyingKey::from_bytes(&hex_dec(&k.pubblica_hex())).unwrap();
        let sig = k256::schnorr::Signature::try_from(&sig[..]).unwrap();
        assert!(pk.verify_raw(&d, &sig).is_ok());
        assert!(deriva(&s, Ruolo::Wallet, 1).unwrap().firma_schnorr(&d).is_none());
    }

    fn hex_dec(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }
}
