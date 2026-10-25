//! NIP-44 v2 con i vettori UFFICIALI (paulmillr/nip44, nip44.vectors.json, sha256 fissato qui sotto):
//! se un solo vettore cambia o non torna, i messaggi non si leggerebbero con gli altri client.

use m4tr1x_dm::nip44::*;
use serde_json::Value;
use sha2::{Digest, Sha256};

const FILE: &str = include_str!("vettori/nip44.vectors.json");
const SHA256_FILE: &str = "269ed0f69e4c192512cc779e78c555090cebc7c785b609e338a62afc3ce25040";

fn vettori() -> Value {
    serde_json::from_str::<Value>(FILE).unwrap()["v2"].clone()
}
fn hex(s: &Value) -> Vec<u8> {
    let t = s.as_str().unwrap();
    (0..t.len() / 2).map(|i| u8::from_str_radix(&t[2 * i..2 * i + 2], 16).unwrap()).collect()
}
fn a32(s: &Value) -> [u8; 32] {
    hex(s).try_into().unwrap()
}
fn esa(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn il_file_dei_vettori_e_quello_ufficiale() {
    assert_eq!(esa(&Sha256::digest(FILE.as_bytes())), SHA256_FILE);
}

#[test]
fn chiave_di_conversazione() {
    let v = vettori();
    let casi = v["valid"]["get_conversation_key"].as_array().unwrap();
    assert!(casi.len() >= 30);
    for c in casi {
        assert_eq!(esa(&chiave_conversazione(&a32(&c["sec1"]), &a32(&c["pub2"])).unwrap()), c["conversation_key"].as_str().unwrap(), "{c}");
    }
}

#[test]
fn chiave_di_conversazione_non_valida_rifiutata() {
    let v = vettori();
    let casi = v["invalid"]["get_conversation_key"].as_array().unwrap();
    assert!(casi.len() >= 8);
    for c in casi {
        assert!(chiave_conversazione(&a32(&c["sec1"]), &a32(&c["pub2"])).is_err(), "{}", c["note"]);
    }
}

#[test]
fn chiavi_del_messaggio() {
    let v = vettori();
    let g = &v["valid"]["get_message_keys"];
    let ck = a32(&g["conversation_key"]);
    let chiavi = g["keys"].as_array().unwrap();
    assert!(chiavi.len() >= 3);
    for k in chiavi {
        let m = chiavi_messaggio(&ck, &a32(&k["nonce"])).unwrap();
        assert_eq!(esa(&m.chacha_key), k["chacha_key"].as_str().unwrap());
        assert_eq!(esa(&m.chacha_nonce), k["chacha_nonce"].as_str().unwrap());
        assert_eq!(esa(&m.hmac_key), k["hmac_key"].as_str().unwrap());
    }
}

#[test]
fn padding() {
    let v = vettori();
    let casi = v["valid"]["calc_padded_len"].as_array().unwrap();
    assert!(casi.len() >= 20);
    for c in casi {
        let (n, p) = (c[0].as_u64().unwrap() as usize, c[1].as_u64().unwrap() as usize);
        assert_eq!(lunghezza_con_padding(n), p, "padding di {n}");
    }
}

#[test]
fn cifra_e_decifra() {
    let v = vettori();
    let casi = v["valid"]["encrypt_decrypt"].as_array().unwrap();
    assert!(casi.len() >= 10);
    for c in casi {
        let ck = a32(&c["conversation_key"]);
        let payload = cifra_con_nonce(&ck, &a32(&c["nonce"]), c["plaintext"].as_str().unwrap()).unwrap();
        assert_eq!(payload, c["payload"].as_str().unwrap());
        assert_eq!(decifra(&ck, c["payload"].as_str().unwrap()).unwrap(), c["plaintext"].as_str().unwrap());
    }
}

#[test]
fn messaggi_lunghi() {
    let v = vettori();
    for c in v["valid"]["encrypt_decrypt_long_msg"].as_array().unwrap() {
        let ck = a32(&c["conversation_key"]);
        let testo = c["pattern"].as_str().unwrap().repeat(c["repeat"].as_u64().unwrap() as usize);
        assert_eq!(esa(&Sha256::digest(testo.as_bytes())), c["plaintext_sha256"].as_str().unwrap());
        let payload = cifra_con_nonce(&ck, &a32(&c["nonce"]), &testo).unwrap();
        assert_eq!(esa(&Sha256::digest(payload.as_bytes())), c["payload_sha256"].as_str().unwrap());
        assert_eq!(decifra(&ck, &payload).unwrap(), testo);
    }
}

#[test]
fn lunghezze_non_ammesse_rifiutate() {
    let v = vettori();
    let ck = [7u8; 32];
    let nonce = [9u8; 32];
    assert!(cifra_con_nonce(&ck, &nonce, "").is_err(), "messaggio vuoto");
    assert!(cifra_con_nonce(&ck, &nonce, &"x".repeat(65536)).is_err(), "oltre 65535 byte");
    assert!(cifra_con_nonce(&ck, &nonce, &"x".repeat(65535)).is_ok(), "65535 byte esatti");
    assert!(!v["invalid"]["encrypt_msg_lengths"].as_array().unwrap().is_empty());
}

#[test]
fn payload_non_valido_rifiutato() {
    let v = vettori();
    let casi = v["invalid"]["decrypt"].as_array().unwrap();
    assert!(casi.len() >= 10);
    for c in casi {
        assert!(decifra(&a32(&c["conversation_key"]), c["payload"].as_str().unwrap()).is_err(), "{}", c["note"]);
    }
}
