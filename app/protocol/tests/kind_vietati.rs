//! I-19: i kind dei messaggi privati "nudi" (4 = DM vecchio NIP-04, 13 = sigillo, 14 = messaggio) non
//! viaggiano mai su un relay: `verifica` e `firma_evento` li rifiutano. Il sigillo vive solo DENTRO la busta
//! cifrata (kind 1059), che invece passa.

use m4tr1x_identity::{deriva, seme_da_parole, Ruolo};
use m4tr1x_protocol::{firma_evento, id_evento, verifica, Rete, Rifiuto};

#[test]
fn i_kind_4_13_14_non_si_firmano_ne_si_accettano_e_il_1059_si() {
    let seme = seme_da_parole(&(["abandon"; 23].join(" ") + " art"), "").unwrap();
    let k = deriva(&seme, Ruolo::Identity, 1).unwrap();
    for kind in [4u16, 13, 14] {
        let r = firma_evento(kind, "ciao", &[], 1_760_000_000, Rete::Prova, &k.pubblica_hex(), |d| k.firma_schnorr(d));
        assert_eq!(r.unwrap_err(), Rifiuto::KindVietato(kind), "kind {kind}");
    }
    for kind in [1u16, 1059] {
        let e = firma_evento(kind, "ciao", &[], 1_760_000_000, Rete::Prova, &k.pubblica_hex(), |d| k.firma_schnorr(d)).expect("controllo positivo");
        let json = serde_json::to_value(&e).unwrap();
        assert!(verifica(&json, Rete::Prova).is_ok(), "kind {kind}");
        // lo stesso evento con il kind cambiato a mano (id e firma restano di prima) non passa comunque
        let mut falso = json.clone();
        falso["kind"] = 14.into();
        assert!(verifica(&falso, Rete::Prova).is_err());
    }
    // un evento di kind 4/13/14 con id e firma GIUSTI (fatto a mano, aggirando firma_evento) non passa lo stesso
    for kind in [4u16, 13, 14] {
        let tags = vec![vec!["net".to_string(), Rete::Prova.nome().to_string()]];
        let id = id_evento(&k.pubblica_hex(), 1_760_000_000, kind, &tags, "ciao");
        let mut digest = [0u8; 32];
        for (i, b) in digest.iter_mut().enumerate() {
            *b = u8::from_str_radix(&id[2 * i..2 * i + 2], 16).unwrap();
        }
        let sig: String = k.firma_schnorr(&digest).unwrap().iter().map(|b| format!("{b:02x}")).collect();
        let ev = serde_json::json!({ "id": id, "pubkey": k.pubblica_hex(), "created_at": 1_760_000_000u64, "kind": kind, "tags": tags, "content": "ciao", "sig": sig });
        assert_eq!(verifica(&ev, Rete::Prova).unwrap_err(), Rifiuto::KindVietato(kind), "kind {kind} firmato bene");
        let mut ok = ev.clone();
        ok["kind"] = 1.into();
        assert!(!matches!(verifica(&ok, Rete::Prova), Err(Rifiuto::KindVietato(_))));
        // controllo positivo: lo stesso evento fatto a mano con un kind permesso (1) passa
        let id1 = id_evento(&k.pubblica_hex(), 1_760_000_000, 1, &tags, "ciao");
        let d1: Vec<u8> = (0..32).map(|i| u8::from_str_radix(&id1[2 * i..2 * i + 2], 16).unwrap()).collect();
        let s1: String = k.firma_schnorr(&d1.try_into().unwrap()).unwrap().iter().map(|b| format!("{b:02x}")).collect();
        let buono = serde_json::json!({ "id": id1, "pubkey": k.pubblica_hex(), "created_at": 1_760_000_000u64, "kind": 1, "tags": tags, "content": "ciao", "sig": s1 });
        assert!(verifica(&buono, Rete::Prova).is_ok(), "il metodo a mano deve produrre eventi validi");
    }
}
