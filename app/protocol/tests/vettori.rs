//! Contratto con il JavaScript: su ogni evento dei vettori CONGELATI
//! (`packages/protocol/vectors/eventi.json`) il lato Rust dà lo stesso esito di `verifyEvent`.
//! Lo lancia anche `tests/invariants/inv-15-no-h8-in-events.test.js` (I-15).

use m4tr1x_protocol::{id_evento, verifica, Rete, Rifiuto};
use serde_json::Value;

const VETTORI: &str = include_str!("../../../packages/protocol/vectors/eventi.json");

/// La firma è salvata in due metà (lo scanner dei segreti segnala 128 esadecimali di fila).
fn evento(v: &Value) -> Value {
    let mut e = v["evento"].clone();
    if let Some(parti) = e["sig"].as_array() {
        let intera: String = parti.iter().map(|p| p.as_str().unwrap()).collect();
        e["sig"] = Value::String(intera);
    }
    e
}

fn vettori() -> Vec<Value> {
    let f: Value = serde_json::from_str(VETTORI).unwrap();
    assert_eq!(f["rete"], "m4tr1x-test");
    let v = f["vettori"].as_array().unwrap().clone();
    assert!(v.len() >= 32, "vettori: {}", v.len());
    v
}

#[test]
fn stesso_esito_del_javascript_su_ogni_vettore() {
    let mut diversi = vec![];
    for v in vettori() {
        let atteso = v["valido"].as_bool().unwrap();
        let esito = verifica(&evento(&v), Rete::Prova);
        if esito.is_ok() != atteso {
            diversi.push(format!("{}: JS {}, Rust {:?}", v["nome"], atteso, esito.err()));
        }
    }
    assert!(diversi.is_empty(), "esiti diversi dal JavaScript:\n{}", diversi.join("\n"));
}

#[test]
fn id_ricalcolato_uguale_sui_vettori_validi() {
    for v in vettori().into_iter().filter(|v| v["valido"] == true) {
        let e = verifica(&evento(&v), Rete::Prova).unwrap();
        assert_eq!(id_evento(&e.pubkey, e.created_at, e.kind, &e.tags, &e.content), v["evento"]["id"], "{}", v["nome"]);
    }
}

#[test]
fn i_rifiuti_hanno_il_motivo_giusto() {
    let per_nome = |n: &str| {
        let v = vettori().into_iter().find(|v| v["nome"] == n).unwrap_or_else(|| panic!("vettore mancante: {n}"));
        verifica(&evento(&v), Rete::Prova).unwrap_err()
    };
    assert!(matches!(per_nome("H8: conto nel contenuto"), Rifiuto::H8(_)));
    assert!(matches!(per_nome("H8: lettera non ASCII prima (in JS è un confine di parola)"), Rifiuto::H8(_)));
    assert!(matches!(per_nome("H8: tag \"H8Account\" vietato (maiuscole)"), Rifiuto::H8(_)));
    assert_eq!(per_nome("rete sbagliata"), Rifiuto::Rete);
    assert_eq!(per_nome("due tag net"), Rifiuto::Rete);
    assert_eq!(per_nome("contenuto cambiato dopo la firma"), Rifiuto::Id);
    assert_eq!(per_nome("firma alterata di un bit"), Rifiuto::Firma);
    assert_eq!(per_nome("chiave pubblica di un altro"), Rifiuto::Id);
    assert_eq!(per_nome("firma valida di un altro evento"), Rifiuto::Firma);
}

#[test]
fn un_evento_della_rete_di_prova_non_vale_sulla_principale() {
    let v = vettori().into_iter().find(|v| v["nome"] == "testo semplice").unwrap();
    assert_eq!(verifica(&evento(&v), Rete::Principale).unwrap_err(), Rifiuto::Rete);
}
