//! I-19: i kind dei messaggi privati "nudi" (4 = DM vecchio NIP-04, 13 = sigillo, 14 = messaggio) non
//! viaggiano mai su un relay: `verifica` e `firma_evento` li rifiutano. Il sigillo vive solo DENTRO la busta
//! cifrata (kind 1059), che invece passa.

use m4tr1x_identity::{deriva, seme_da_parole, Ruolo};
use m4tr1x_protocol::{firma_evento, verifica, Rete, Rifiuto};

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
}
