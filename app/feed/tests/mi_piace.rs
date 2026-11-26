//! I-69: i "mi piace" si contano solo se verificati, una persona una volta, solo per i contenuti chiesti.
use m4tr1x_feed::conta_mi_piace;
use m4tr1x_protocol::Rete;
use serde_json::Value;

fn chiave(versione: u32) -> m4tr1x_identity::ChiaveRuolo {
    use m4tr1x_identity::{deriva, seme_da_parole, Ruolo};
    let seme = seme_da_parole(&(["abandon"; 23].join(" ") + " art"), "").unwrap();
    deriva(&seme, Ruolo::Identity, versione).unwrap()
}

fn reazione(k: &m4tr1x_identity::ChiaveRuolo, testo: &str, tags: &[Vec<String>], quando: u64, rete: Rete) -> Value {
    serde_json::to_value(m4tr1x_protocol::firma_evento(7, testo, tags, quando, rete, &k.pubblica_hex(), |d| k.firma_schnorr(d)).unwrap()).unwrap()
}

fn e(id: &str) -> Vec<Vec<String>> {
    vec![vec!["e".into(), id.into()]]
}

#[test]
fn solo_reazioni_valide_una_per_persona() {
    let (a, b, c) = (chiave(1), chiave(2), chiave(3));
    let uno = "1".repeat(64);
    let due = "2".repeat(64);
    let estraneo = "3".repeat(64);
    let mia_vecchia = reazione(&a, "+", &e(&uno), 100, Rete::Prova);
    let mia_nuova = reazione(&a, "+", &e(&uno), 200, Rete::Prova);
    let id_mia_nuova = mia_nuova["id"].as_str().unwrap().to_string();
    let mut falsa = reazione(&c, "+", &e(&uno), 100, Rete::Prova);
    falsa["content"] = "+ ".into(); // firma non più valida
    let grezzi = vec![
        mia_vecchia,
        mia_nuova,
        reazione(&b, "+", &e(&uno), 100, Rete::Prova),
        reazione(&b, "+", &e(&uno), 101, Rete::Prova), // la stessa persona due volte
        reazione(&c, "-", &e(&uno), 100, Rete::Prova), // non è un mi piace
        reazione(&c, "+", &[vec!["e".into(), uno.clone()], vec!["e".into(), due.clone()]], 100, Rete::Prova), // due contenuti
        reazione(&c, "+", &e(&estraneo), 100, Rete::Prova), // contenuto non chiesto
        reazione(&c, "+", &e(&uno), 100, Rete::Principale), // altra rete
        falsa,
        reazione(&c, "+", &e(&due), 100, Rete::Prova),
    ];
    let r = conta_mi_piace(grezzi, Rete::Prova, &[uno.clone(), due.clone()], Some(&a.pubblica_hex()));
    assert_eq!(r[&uno].quante, 2, "contati male: {:?}", r[&uno]);
    assert_eq!(r[&uno].mia.as_deref(), Some(id_mia_nuova.as_str()), "la mia reazione non è la più recente");
    assert_eq!(r[&due].quante, 1);
    assert_eq!(r[&due].mia, None, "la reazione di un altro presa per mia");
    assert!(!r.contains_key(&estraneo), "contato un contenuto non chiesto");
}
