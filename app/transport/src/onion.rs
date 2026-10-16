//! Indirizzi onion v3 validati (rend-spec-v3, sezione "Encoding onion addresses").
//!
//! `OnionV3` si costruisce SOLO da una stringa che passa tutti i controlli:
//! 56 caratteri base32 + ".onion", versione 3, checksum giusto. Non esiste un modo
//! per ottenere un `OnionV3` da un nome in chiaro o da un IP: è il tipo stesso a
//! rendere impossibile chiedere al trasporto una connessione fuori dagli onion (I-36).
//!
//!   indirizzo = base32(PUBKEY[32] | CHECKSUM[2] | VERSION[1]) + ".onion"
//!   CHECKSUM  = SHA3-256(".onion checksum" | PUBKEY | VERSION)[..2]

use data_encoding::BASE32_NOPAD;
use sha3::{Digest, Sha3_256};
use std::fmt;

const VERSION: u8 = 0x03;
const B32_LEN: usize = 56;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OnionError {
    #[error("non termina con .onion")]
    NoSuffix,
    #[error("sottodomini non ammessi")]
    Subdomain,
    #[error("lunghezza sbagliata: servono 56 caratteri prima di .onion")]
    Length,
    #[error("caratteri non base32")]
    Base32,
    #[error("versione {0} non supportata: solo onion v3")]
    Version(u8),
    #[error("checksum sbagliato")]
    Checksum,
}

/// Indirizzo onion v3 già validato. Clonabile, confrontabile, mai costruibile a mano.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct OnionV3 {
    pubkey: [u8; 32],
}

fn checksum(pubkey: &[u8; 32]) -> [u8; 2] {
    let mut h = Sha3_256::new();
    h.update(b".onion checksum");
    h.update(pubkey);
    h.update([VERSION]);
    let d = h.finalize();
    [d[0], d[1]]
}

impl OnionV3 {
    /// Valida un nome host. Maiuscole accettate (i nomi DNS non distinguono), output sempre minuscolo.
    pub fn parse(host: &str) -> Result<Self, OnionError> {
        let lower = host.to_ascii_lowercase();
        let label = lower.strip_suffix(".onion").ok_or(OnionError::NoSuffix)?;
        if label.contains('.') {
            return Err(OnionError::Subdomain);
        }
        if label.len() != B32_LEN {
            return Err(OnionError::Length);
        }
        let raw = BASE32_NOPAD
            .decode(label.to_ascii_uppercase().as_bytes())
            .map_err(|_| OnionError::Base32)?;
        // 56 caratteri base32 = 35 byte esatti
        let (pk, rest) = raw.split_at(32);
        let pubkey: [u8; 32] = pk.try_into().map_err(|_| OnionError::Length)?;
        if rest[2] != VERSION {
            return Err(OnionError::Version(rest[2]));
        }
        if rest[..2] != checksum(&pubkey) {
            return Err(OnionError::Checksum);
        }
        Ok(Self { pubkey })
    }

    /// Costruisce l'indirizzo da una chiave pubblica ed25519 del servizio.
    pub fn from_pubkey(pubkey: [u8; 32]) -> Self {
        Self { pubkey }
    }

    pub fn pubkey(&self) -> &[u8; 32] {
        &self.pubkey
    }

    /// Nome host completo, minuscolo, con ".onion".
    pub fn host(&self) -> String {
        let mut raw = Vec::with_capacity(35);
        raw.extend_from_slice(&self.pubkey);
        raw.extend_from_slice(&checksum(&self.pubkey));
        raw.push(VERSION);
        format!("{}.onion", BASE32_NOPAD.encode(&raw).to_ascii_lowercase())
    }
}

impl fmt::Display for OnionV3 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.host())
    }
}

impl fmt::Debug for OnionV3 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // nei log solo un prefisso: gli onion dei nodi non vanno ripetuti per intero
        write!(f, "OnionV3({}…)", &self.host()[..8])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gli onion non si scrivono nei sorgenti (lo scanner I-28 li blocca): si costruiscono da chiavi di prova.
    fn sample() -> OnionV3 {
        OnionV3::from_pubkey([7u8; 32])
    }

    fn swap_char(s: &str, i: usize) -> String {
        let mut b = s.as_bytes().to_vec();
        b[i] = if b[i] == b'a' { b'b' } else { b'a' };
        String::from_utf8(b).unwrap()
    }

    #[test]
    fn andata_e_ritorno() {
        let o = sample();
        let host = o.host();
        assert_eq!(host.len(), 62);
        assert_eq!(OnionV3::parse(&host).unwrap(), o);
        assert_eq!(OnionV3::parse(&host.to_ascii_uppercase()).unwrap(), o);
    }

    /// Vettore calcolato da un'implementazione indipendente (Python, dalla specifica),
    /// che riconosce valido anche un onion reale del Tor Project. Si confronta l'hash
    /// dell'host perché gli onion non si scrivono nei sorgenti.
    #[test]
    fn vettore_indipendente() {
        let h = Sha3_256::digest(sample().host().as_bytes());
        let hex: String = h.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "eaab160b4bfd99db08f7576dfd60b119a8d069a0eee9bad440937d5d58661418");
    }

    #[test]
    fn rifiuta_host_in_chiaro() {
        for h in ["example.com", "localhost", "127.0.0.1", "192.0.2.1", "[::1]", "", ".onion"] {
            assert!(OnionV3::parse(h).is_err(), "accettato {h:?}");
        }
    }

    #[test]
    fn rifiuta_onion_v2_e_lunghezze_sbagliate() {
        let host = sample().host();
        let label = &host[..56];
        assert_eq!(OnionV3::parse(&format!("{}.onion", &label[..16])), Err(OnionError::Length));
        assert_eq!(OnionV3::parse(&format!("{}a.onion", label)), Err(OnionError::Length));
    }

    #[test]
    fn rifiuta_checksum_sbagliato() {
        let host = sample().host();
        // cambia un carattere della chiave: il checksum non torna più
        assert_eq!(OnionV3::parse(&swap_char(&host, 3)), Err(OnionError::Checksum));
    }

    #[test]
    fn rifiuta_versione_diversa_da_3() {
        let pk = [7u8; 32];
        let mut raw = pk.to_vec();
        raw.extend_from_slice(&checksum(&pk));
        raw.push(0x02);
        let host = format!("{}.onion", BASE32_NOPAD.encode(&raw).to_ascii_lowercase());
        assert_eq!(OnionV3::parse(&host), Err(OnionError::Version(2)));
    }

    #[test]
    fn rifiuta_sottodomini_e_caratteri_non_base32() {
        let host = sample().host();
        assert_eq!(OnionV3::parse(&format!("www.{host}")), Err(OnionError::Subdomain));
        let mut bad = host.clone().into_bytes();
        bad[0] = b'1'; // '1' non è nell'alfabeto base32
        assert_eq!(OnionV3::parse(std::str::from_utf8(&bad).unwrap()), Err(OnionError::Base32));
    }

    #[test]
    fn debug_non_stampa_l_indirizzo_intero() {
        let o = sample();
        let d = format!("{o:?}");
        assert!(!d.contains(&o.host()[..56]));
    }
}
