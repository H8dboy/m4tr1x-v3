//! I-73: una ricondivisione compare solo se è verificata E il post originale è verificato.
use m4tr1x_feed::{con_ricondivisioni, ricondivisioni, verificati};
use m4tr1x_protocol::Rete;
use serde_json::Value;

fn chiave(versione: u32) -> m4tr1x_identity::ChiaveRuolo {
    use m4tr1x_identity::{deriva, seme_da_parole, Ruolo};
    let seme = seme_da_parole(&(["abandon"; 23].join(" ") + " art"), "").unwrap();
    deriva(&seme, Ruolo::Identity, versione).unwrap()
}

fn ev(k: &m4tr1x_identity::ChiaveRuolo, kind: u16, testo: &str, tags: &[Vec<String>], quando: u64) -> Value {
    serde_json::to_value(m4tr1x_protocol::firma_evento(kind, testo, tags, quando, Rete::Prova, &k.pubblica_hex(), |d| k.firma_schnorr(d)).unwrap()).unwrap()
}

#[test]
fn solo_ricondivisioni_verificate_di_originali_verificati() {
    let (anna, bruno, carla) = (chiave(1), chiave(2), chiave(3));
    let nel_feed = ev(&anna, 1, "nel feed", &[], 100);
    let fuori = ev(&anna, 1, "più vecchio, chiesto per id", &[], 50);
    let mut falso_orig = ev(&anna, 1, "originale falso", &[], 60);
    let id = |v: &Value| v["id"].as_str().unwrap().to_string();
    let r = |k, orig: &Value, q| ev(k, 6, "", &m4tr1x_protocol::tag_ricondividi(&id(orig)).unwrap(), q);
    let r1 = r(&bruno, &nel_feed, 300);
    let r2 = r(&carla, &fuori, 200);
    let r3 = r(&carla, &falso_orig, 250);
    falso_orig["content"] = "testo cambiato".into(); // firma non più valida
    let mut r_falsa = r(&bruno, &fuori, 400);
    r_falsa["created_at"] = 999.into();
    let r_se = r(&anna, &nel_feed, 350); // anna ricondivide sé stessa: niente voce
    let r1_vecchia = r(&bruno, &nel_feed, 150); // doppione più vecchio della stessa persona
    // carla su un'altra rete: firma valida, ma non è questa rete
    let altra_rete = serde_json::to_value(m4tr1x_protocol::firma_evento(6, "", &m4tr1x_protocol::tag_ricondividi(&id(&nel_feed)).unwrap(), 600, Rete::Principale, &carla.pubblica_hex(), |d| carla.firma_schnorr(d)).unwrap()).unwrap();
    let mancante = ev(&bruno, 6, "", &m4tr1x_protocol::tag_ricondividi(&"f".repeat(64)).unwrap(), 500);

    let ric = ricondivisioni(&[r1.clone(), r2.clone(), r3, r_falsa, r_se, r1_vecchia, mancante, altra_rete], Rete::Prova);
    let (post, _) = verificati(vec![nel_feed.clone()], Rete::Prova);
    let (originali, _) = verificati(vec![fuori.clone(), falso_orig], Rete::Prova);
    let feed = con_ricondivisioni(post, ric, originali);
    let visto: Vec<(String, Option<String>, Option<String>)> = feed.iter().map(|p| (p.id.clone(), p.ricondivisione.as_ref().map(|r| r.id.clone()), p.ricondivisione.as_ref().map(|r| r.da.clone()))).collect();
    assert_eq!(
        visto,
        vec![
            (id(&nel_feed), Some(id(&r1)), Some(bruno.pubblica_hex())),
            (id(&fuori), Some(id(&r2)), Some(carla.pubblica_hex())),
            (id(&nel_feed), None, None),
        ],
        "feed con ricondivisioni sbagliato"
    );
    // il testo viene dall'originale verificato, non dalla ricondivisione
    assert_eq!(feed[1].testo, "più vecchio, chiesto per id");
}
