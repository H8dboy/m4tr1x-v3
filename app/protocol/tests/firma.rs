//! Costruttore degli eventi nel lato Rust (F4, post): firma con l'identità, dati H8 rifiutati PRIMA di
//! firmare (I-15), evento riverificato prima di uscire. La chiave è quella del seme PUBBLICO di prova
//! (lo stesso di tests/unit/keys-vectors.test.js: non è un segreto).

use m4tr1x_identity::{deriva, seme_da_parole, ChiaveRuolo, Ruolo};
use m4tr1x_protocol::{firma_evento, verifica, Rete, Rifiuto};
use std::cell::Cell;

/// identity v1 congelata in tests/unit/keys-vectors.test.js
const IDENTITY_V1: &str = "817d2b662e9c2c6e080c5fd5d9c4866a8342d194dbea7f98c1f4fc30e069bed7";

fn chiave(versione: u32) -> ChiaveRuolo {
    let seme = seme_da_parole(&(["abandon"; 23].join(" ") + " art"), "").unwrap();
    deriva(&seme, Ruolo::Identity, versione).unwrap()
}

#[test]
fn post_firmato_con_l_identita_e_verificabile() {
    let k = chiave(1);
    assert_eq!(k.pubblica_hex(), IDENTITY_V1);
    let testo = "Nderë 🦅 — \"virgolette\" \\ a capo\n ctrl \u{1} sep \u{2028} </script>";
    let e = firma_evento(1, testo, &[vec!["t".into(), "città".into()]], 1_760_000_000, Rete::Prova, &k.pubblica_hex(), |d| k.firma_schnorr(d)).unwrap();
    assert_eq!(e.pubkey, IDENTITY_V1);
    assert_eq!(e.content, testo);
    assert_eq!(e.tags.last().unwrap(), &vec!["net".to_string(), "m4tr1x-test".to_string()]);
    let json = serde_json::to_value(&e).unwrap();
    assert!(verifica(&json, Rete::Prova).is_ok());
    assert_eq!(verifica(&json, Rete::Principale).unwrap_err(), Rifiuto::Rete);
}

#[test]
fn dati_h8_rifiutati_prima_di_firmare() {
    let k = chiave(1);
    let firmato = Cell::new(false);
    let conto = format!("m4a1{}", "0123456789abcdef".repeat(2));
    for (testo, tags) in [
        (format!("paga {conto}"), vec![]),
        ("ok".to_string(), vec![vec!["wallet".to_string(), "x".to_string()]]),
        ("ok".to_string(), vec![vec!["t".to_string(), conto.clone()]]),
    ] {
        let e = firma_evento(1, &testo, &tags, 1, Rete::Prova, &k.pubblica_hex(), |d| {
            firmato.set(true);
            k.firma_schnorr(d)
        });
        assert!(matches!(e, Err(Rifiuto::H8(_))), "{testo:?} {tags:?}: {e:?}");
    }
    assert!(!firmato.get(), "la chiave ha firmato un evento con dati H8");
}

#[test]
fn tag_net_lo_mette_solo_il_protocollo() {
    let k = chiave(1);
    let e = firma_evento(1, "ok", &[vec!["net".into(), "m4tr1x-main".into()]], 1, Rete::Prova, &k.pubblica_hex(), |d| k.firma_schnorr(d));
    assert!(matches!(e, Err(Rifiuto::Forma(_))), "{e:?}");
}

/// La riverifica prende una firma della chiave sbagliata (es. portachiavi con un'altra versione).
#[test]
fn firma_di_un_altra_chiave_non_esce() {
    let (k1, k2) = (chiave(1), chiave(2));
    let e = firma_evento(1, "ok", &[], 1, Rete::Prova, &k1.pubblica_hex(), |d| k2.firma_schnorr(d));
    assert_eq!(e.unwrap_err(), Rifiuto::Firma);
}
