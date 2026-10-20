//! Eventi M4TR1X nel lato Rust dell'app (F4): stessi esiti di `packages/protocol` (JavaScript).
//!
//! Un evento è un evento Nostr (NIP-01) con in più il tag `["net", <rete>]` (I-08): id =
//! SHA-256 di `[0, pubkey, created_at, kind, tags, content]` serializzato come `JSON.stringify`,
//! firma BIP-340 (Schnorr secp256k1) dell'id.
//!
//! Il contratto con il JavaScript sono i vettori CONGELATI `packages/protocol/vectors/eventi.json`
//! (test `tests/vettori.rs`): stesso esito su ogni evento. Qui si è più severi del JavaScript,
//! mai più permissivi: `created_at` intero tra 0 e 2^53−1 scritto senza decimali, nessun
//! surrogato spaiato (Rust non li rappresenta).
//!
//! Dati H8 vietati negli eventi (I-15): stessa regola di `h8Violation`, scritta a mano senza la
//! libreria `regex`: il confine di parola `\b` del JavaScript conosce solo le lettere ASCII,
//! quello di `regex` anche le altre, e su "àm4a1…" i due darebbero esiti diversi.

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Reti ammesse (stesse di `NETWORKS` in packages/protocol).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rete {
    Principale,
    Prova,
}

impl Rete {
    pub fn nome(self) -> &'static str {
        match self {
            Rete::Principale => "m4tr1x-main",
            Rete::Prova => "m4tr1x-test",
        }
    }
}

/// Più grande intero che `JSON.stringify` scrive esatto (Number.MAX_SAFE_INTEGER).
const MAX_INTERO_JS: u64 = (1 << 53) - 1;

/// Evento già verificato: l'unico modo di averne uno è [`verifica`].
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Evento {
    pub id: String,
    pub pubkey: String,
    pub created_at: u64,
    pub kind: u16,
    pub tags: Vec<Vec<String>>,
    pub content: String,
    pub sig: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Rifiuto {
    #[error("forma non valida: {0}")]
    Forma(&'static str),
    #[error("rete sbagliata o tag net mancante/doppio")]
    Rete,
    #[error("dati H8 nell'evento: {0}")]
    H8(&'static str),
    #[error("id non corrispondente al contenuto")]
    Id,
    #[error("firma non valida")]
    Firma,
}

/// Verifica completa di un evento ricevuto (forma, rete, dati H8, id, firma), come `verifyEvent`.
pub fn verifica(v: &Value, rete: Rete) -> Result<Evento, Rifiuto> {
    let o = v.as_object().ok_or(Rifiuto::Forma("non è un oggetto"))?;
    let testo = |k: &'static str| o.get(k).and_then(Value::as_str).ok_or(Rifiuto::Forma(k));
    let id = testo("id")?;
    let pubkey = testo("pubkey")?;
    let sig = testo("sig")?;
    if !esadecimale_minuscolo(id, 64) || !esadecimale_minuscolo(pubkey, 64) || !esadecimale_minuscolo(sig, 128) {
        return Err(Rifiuto::Forma("id, pubkey o sig"));
    }
    let created_at = intero(o.get("created_at")).filter(|&n| n <= MAX_INTERO_JS).ok_or(Rifiuto::Forma("created_at"))?;
    let kind = intero(o.get("kind")).and_then(|n| u16::try_from(n).ok()).ok_or(Rifiuto::Forma("kind"))?;
    let content = testo("content")?;
    let tags = o
        .get("tags")
        .and_then(Value::as_array)
        .ok_or(Rifiuto::Forma("tags"))?
        .iter()
        .map(|t| {
            let t = t.as_array().filter(|t| !t.is_empty()).ok_or(Rifiuto::Forma("tag vuoto o non lista"))?;
            t.iter().map(|x| x.as_str().map(str::to_owned).ok_or(Rifiuto::Forma("tag con valori non testo"))).collect()
        })
        .collect::<Result<Vec<Vec<String>>, _>>()?;

    let reti: Vec<_> = tags.iter().filter(|t| t[0] == "net").collect();
    if reti.len() != 1 || reti[0].get(1).map(String::as_str) != Some(rete.nome()) {
        return Err(Rifiuto::Rete);
    }
    if let Some(motivo) = violazione_h8(content, &tags) {
        return Err(Rifiuto::H8(motivo));
    }
    if id_evento(pubkey, created_at, kind, &tags, content) != id {
        return Err(Rifiuto::Id);
    }
    verifica_firma(pubkey, id, sig)?;
    Ok(Evento {
        id: id.to_owned(),
        pubkey: pubkey.to_owned(),
        created_at,
        kind,
        tags,
        content: content.to_owned(),
        sig: sig.to_owned(),
    })
}

/// Id dell'evento in esadecimale: SHA-256 della serializzazione NIP-01.
/// `serde_json` scrive le stringhe come `JSON.stringify` (stessi caratteri scappati, stesso
/// `\u00XX` minuscolo, niente `\/`, U+2028 lasciato com'è): lo provano i vettori.
pub fn id_evento(pubkey: &str, created_at: u64, kind: u16, tags: &[Vec<String>], content: &str) -> String {
    let testo = serde_json::to_string(&(0u8, pubkey, created_at, kind, tags, content)).expect("serializzazione di tipi semplici");
    esadecimale(&Sha256::digest(testo.as_bytes()))
}

fn verifica_firma(pubkey: &str, id: &str, sig: &str) -> Result<(), Rifiuto> {
    use k256::schnorr::{Signature, VerifyingKey};
    let chiave = VerifyingKey::from_bytes(&da_esadecimale(pubkey).ok_or(Rifiuto::Firma)?).map_err(|_| Rifiuto::Firma)?;
    let firma = Signature::try_from(da_esadecimale(sig).ok_or(Rifiuto::Firma)?.as_slice()).map_err(|_| Rifiuto::Firma)?;
    chiave.verify_raw(&da_esadecimale(id).ok_or(Rifiuto::Firma)?, &firma).map_err(|_| Rifiuto::Firma)
}

// ─── Dati H8 vietati (I-15) ──────────────────────────────────────────────────

const TAG_VIETATI: [&str; 5] = ["h8", "h8host", "wallet", "account", "h8account"];

/// Come `h8Violation` di packages/protocol: motivo, o None se l'evento è pulito.
pub fn violazione_h8(content: &str, tags: &[Vec<String>]) -> Option<&'static str> {
    for t in tags {
        if TAG_VIETATI.contains(&t[0].to_lowercase().as_str()) {
            return Some("tag vietato (dati H8)");
        }
        if t.iter().any(|v| contiene_h8(v)) {
            return Some("conto o indirizzo H8 in un tag");
        }
    }
    if contiene_h8(content) {
        return Some("conto o indirizzo H8 nel contenuto");
    }
    None
}

/// `/\bm4a1[0-9a-f]{32}\b/i` oppure `/\bH8[0-9a-f]{38}\b/i`, con il `\b` del JavaScript.
fn contiene_h8(s: &str) -> bool {
    trova(s.as_bytes(), b"m4a1", 32) || trova(s.as_bytes(), b"h8", 38)
}

/// Lettera di parola per il `\b` del JavaScript: solo [A-Za-z0-9_]. I byte non ASCII dell'UTF-8
/// non lo sono mai, come i caratteri non ASCII per il JavaScript.
fn di_parola(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn trova(s: &[u8], prefisso: &[u8], cifre: usize) -> bool {
    let n = prefisso.len() + cifre;
    (0..s.len().saturating_sub(n - 1)).any(|i| {
        let pezzo = &s[i..i + n];
        (i == 0 || !di_parola(s[i - 1]))
            && pezzo[..prefisso.len()].eq_ignore_ascii_case(prefisso)
            && pezzo[prefisso.len()..].iter().all(u8::is_ascii_hexdigit)
            && s.get(i + n).is_none_or(|&b| !di_parola(b))
    })
}

// ─── Piccoli aiuti ───────────────────────────────────────────────────────────

/// Intero non negativo scritto senza decimali (1.0 e -1 si rifiutano).
fn intero(v: Option<&Value>) -> Option<u64> {
    v.and_then(Value::as_u64)
}

fn esadecimale_minuscolo(s: &str, lunghezza: usize) -> bool {
    s.len() == lunghezza && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn esadecimale(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn da_esadecimale(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confini_di_parola_come_il_javascript() {
        let h = "0123456789abcdef".repeat(2);
        assert!(contiene_h8(&format!("m4a1{h}")));
        assert!(contiene_h8(&format!("(M4A1{})", h.to_uppercase())));
        assert!(contiene_h8(&format!("àm4a1{h}")), "lettera non ASCII prima: in JS è un confine");
        assert!(contiene_h8(&format!("m4a1{h}é")), "lettera non ASCII dopo: in JS è un confine");
        assert!(!contiene_h8(&format!("xm4a1{h}")));
        assert!(!contiene_h8(&format!("m4a1{h}_")));
        assert!(!contiene_h8(&format!("m4a1{h}0")));
        assert!(!contiene_h8(&format!("m4a1{}", &h[1..])));
        assert!(!contiene_h8("m4a1"));
        assert!(!contiene_h8(""));
    }

    #[test]
    fn created_at_oltre_gli_interi_esatti_del_javascript_si_rifiuta() {
        assert_eq!(intero(Some(&serde_json::json!(1.0))), None);
        assert_eq!(intero(Some(&serde_json::json!(-1))), None);
        assert_eq!(intero(Some(&serde_json::json!(7))), Some(7));
    }
}
