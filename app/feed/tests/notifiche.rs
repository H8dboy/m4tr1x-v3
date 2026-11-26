//! I-71: notifiche calcolate sul dispositivo, solo da eventi verificati, solo sui TUOI contenuti, mai le tue.
use m4tr1x_feed::{miei_contenuti, notifiche_da, TipoNotifica};
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
fn solo_risposte_e_mi_piace_verificati_ai_miei_contenuti() {
    let (io, altro, terzo) = (chiave(1), chiave(2), chiave(3));
    let mia = io.pubblica_hex();
    let disc = ev(&io, 1, "una discussione", &m4tr1x_protocol::tag_discussione("titolo").unwrap(), 100);
    let post = ev(&io, 1, "un post", &[], 101);
    let suo = ev(&altro, 1, "di un altro", &[], 102);
    let miei = miei_contenuti(&[disc.clone(), post.clone(), suo.clone()], Rete::Prova, &mia);
    let id_disc = disc["id"].as_str().unwrap().to_string();
    let id_post = post["id"].as_str().unwrap().to_string();
    let id_suo = suo["id"].as_str().unwrap().to_string();
    assert_eq!(miei.len(), 2, "contato come mio un contenuto di un altro");
    assert!(miei.contains(&id_disc) && miei.contains(&id_post));

    let e = |id: &str| vec![vec!["e".to_string(), id.to_string()]];
    let risposta = ev(&altro, 1, "ti rispondo", &m4tr1x_protocol::tag_risposta(&id_disc).unwrap(), 200);
    let piace = ev(&terzo, 7, "+", &e(&id_post), 210);
    let mut falso = ev(&terzo, 7, "+", &e(&id_disc), 220);
    falso["created_at"] = 999.into(); // firma non più valida
    let grezzi = vec![
        risposta.clone(),
        risposta.clone(),                                                          // doppione
        piace.clone(),
        falso,
        ev(&io, 7, "+", &e(&id_post), 230),                                        // il mio mi piace
        ev(&io, 1, "mi rispondo", &m4tr1x_protocol::tag_risposta(&id_disc).unwrap(), 240), // la mia risposta
        ev(&terzo, 7, "+", &e(&id_suo), 250),                                      // a un contenuto non mio
        ev(&terzo, 7, "-", &e(&id_post), 260),                                     // non è un mi piace
        ev(&terzo, 1, "post qualsiasi", &e(&id_post), 270),                        // non è una risposta del forum
    ];
    let n = notifiche_da(grezzi, Rete::Prova, &mia, &miei);
    let visto: Vec<(TipoNotifica, &str, &str)> = n.iter().map(|x| (x.tipo, x.id.as_str(), x.contenuto.as_str())).collect();
    assert_eq!(
        visto,
        vec![(TipoNotifica::MiPiace, piace["id"].as_str().unwrap(), id_post.as_str()), (TipoNotifica::Risposta, risposta["id"].as_str().unwrap(), id_disc.as_str())],
        "notifiche sbagliate"
    );
    assert_eq!(n[0].autore, terzo.pubblica_hex());
}
