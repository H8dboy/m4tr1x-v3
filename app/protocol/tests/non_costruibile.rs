//! Un `Evento` non si costruisce da fuori del protocollo: lo prova il doctest `compile_fail` su `Evento`
//! in src/lib.rs. Qui si controlla che il campo privato non cambi il JSON verso l'interfaccia (NIP-01).

use m4tr1x_identity::{deriva, seme_da_parole, Ruolo};
use m4tr1x_protocol::{firma_evento, Rete};

#[test]
fn il_campo_privato_non_esce_nel_json() {
    let seme = seme_da_parole(&(["abandon"; 23].join(" ") + " art"), "").unwrap();
    let k = deriva(&seme, Ruolo::Identity, 1).unwrap();
    let e = firma_evento(1, "x", &[], 1, Rete::Prova, &k.pubblica_hex(), |d| k.firma_schnorr(d)).unwrap();
    let v = serde_json::to_value(&e).unwrap();
    let mut chiavi: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    chiavi.sort();
    assert_eq!(chiavi, ["content", "created_at", "id", "kind", "pubkey", "sig", "tags"]);
}
