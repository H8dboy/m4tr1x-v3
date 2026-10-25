//! I-60 — Un messaggio si attribuisce a un'identità SOLO se l'annuncio più recente di quell'identità
//! (evento firmato dall'identità) elenca la chiave DM che l'ha sigillato, con la prova di possesso firmata
//! da quella stessa chiave DM. Altrimenti il mittente resta sconosciuto: mai un nome inventato.

mod comune;
use comune::*;
use m4tr1x_dm::{annuncio_dm, apri, dm_dichiarate, identita_del_mittente, incarta, EntropiaSistema, KIND_ANNUNCIO_DM};
use m4tr1x_protocol::{firma_evento, Rete};
use serde_json::{json, Value};

const R: Rete = Rete::Prova;
const RELAY: &str = "relay-di-prova-1";

fn annuncio(u: &Utente, dm: &[&m4tr1x_identity::ChiaveRuolo], ora: u64) -> Value {
    serde_json::to_value(annuncio_dm(R, &u.identita, dm, &[RELAY.to_string()], ora).unwrap()).unwrap()
}
fn dm_di_anna_come_mittente() -> String {
    let (a, b) = (anna(), bruno());
    let buste = incarta(R, &a.dm, &b.dm.pubblica_hex(), "ciao", ORA, &mut EntropiaSistema).unwrap();
    apri(R, &b.dm, &serde_json::to_value(&buste.per_destinatario).unwrap()).unwrap().dm_mittente
}

#[test]
fn l_annuncio_e_un_evento_firmato_dall_identita_con_la_prova_di_possesso() {
    let a = anna();
    let ev = annuncio(&a, &[&a.dm], ORA);
    assert_eq!(ev["kind"], KIND_ANNUNCIO_DM);
    assert_eq!(ev["pubkey"], a.identita.pubblica_hex(), "firmato dall'identità, non dalla chiave DM");
    assert!(m4tr1x_protocol::verifica(&ev, R).is_ok());
    let (identita, chiavi) = dm_dichiarate(R, &ev).unwrap();
    assert_eq!(identita, a.identita.pubblica_hex());
    assert_eq!(chiavi, vec![a.dm.pubblica_hex()]);
    // il relay dei DM è dichiarato con un tag relay
    assert!(ev["tags"].as_array().unwrap().iter().any(|t| t[0] == "relay" && t[1] == RELAY));
}

#[test]
fn con_l_annuncio_giusto_il_messaggio_e_di_anna() {
    let a = anna();
    let dm = dm_di_anna_come_mittente();
    assert_eq!(identita_del_mittente(R, &[annuncio(&a, &[&a.dm], ORA)], &dm), Some(a.identita.pubblica_hex()));
}

#[test]
fn senza_annuncio_il_mittente_resta_sconosciuto() {
    let dm = dm_di_anna_come_mittente();
    assert_eq!(identita_del_mittente(R, &[], &dm), None);
    // l'annuncio di un'altra persona non c'entra
    let b = bruno();
    assert_eq!(identita_del_mittente(R, &[annuncio(&b, &[&b.dm], ORA)], &dm), None);
}

#[test]
fn un_impostore_non_si_prende_la_chiave_dm_di_un_altro() {
    // Bruno annuncia la chiave DM di Anna come se fosse sua: non può produrre la prova di possesso (la firma
    // della chiave DM di Anna) → l'annuncio non conta per quella chiave, e Anna resta Anna.
    let (a, b) = (anna(), bruno());
    let dm = dm_di_anna_come_mittente();
    let mut falso = annuncio(&b, &[&b.dm], ORA + 10);
    let tag_dm_anna = annuncio(&a, &[&a.dm], ORA)["tags"].as_array().unwrap().iter().find(|t| t[0] == "dm").unwrap().clone();
    // si sostituisce il tag dm di Bruno con quello COPIATO da Anna (chiave + prova di Anna) e si rifirma come Bruno
    let mut tags: Vec<Vec<String>> = falso["tags"].as_array().unwrap().iter().filter(|t| t[0] != "dm" && t[0] != "net").map(|t| t.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect()).collect();
    tags.push(tag_dm_anna.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect());
    let ev = firma_evento(KIND_ANNUNCIO_DM, "", &tags, ORA + 10, R, &b.identita.pubblica_hex(), |d| b.identita.firma_schnorr(d)).unwrap();
    falso = serde_json::to_value(&ev).unwrap();
    assert!(dm_dichiarate(R, &falso).map(|(_, k)| k.is_empty()).unwrap_or(true), "la prova di Anna non vale per l'identità di Bruno");
    assert_eq!(identita_del_mittente(R, &[falso.clone(), annuncio(&a, &[&a.dm], ORA)], &dm), Some(a.identita.pubblica_hex()));
    assert_eq!(identita_del_mittente(R, &[falso], &dm), None);
}

#[test]
fn prova_di_possesso_alterata_o_mancante_non_vale() {
    let a = anna();
    let dm = dm_di_anna_come_mittente();
    let buono = annuncio(&a, &[&a.dm], ORA);
    // senza prova: tag ["dm", chiave] e basta
    let tags: Vec<Vec<String>> = buono["tags"].as_array().unwrap().iter().filter(|t| t[0] != "net").map(|t| {
        let mut v: Vec<String> = t.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
        if v[0] == "dm" { v.truncate(2) }
        v
    }).collect();
    let senza = serde_json::to_value(firma_evento(KIND_ANNUNCIO_DM, "", &tags, ORA, R, &a.identita.pubblica_hex(), |d| a.identita.firma_schnorr(d)).unwrap()).unwrap();
    assert_eq!(identita_del_mittente(R, &[senza], &dm), None, "tag dm senza prova di possesso");
    // prova con una firma sbagliata (tutta a zero)
    let tags2: Vec<Vec<String>> = tags.iter().map(|t| if t[0] == "dm" { vec!["dm".into(), t[1].clone(), "0".repeat(128)] } else { t.clone() }).collect();
    let alterata = serde_json::to_value(firma_evento(KIND_ANNUNCIO_DM, "", &tags2, ORA, R, &a.identita.pubblica_hex(), |d| a.identita.firma_schnorr(d)).unwrap()).unwrap();
    assert_eq!(identita_del_mittente(R, &[alterata], &dm), None, "prova di possesso falsa");
}

#[test]
fn vale_l_annuncio_piu_recente_della_stessa_identita() {
    // Anna ruota la chiave DM: v1 → v2. L'annuncio nuovo elenca SOLO la v2: la v1 non è più attribuita.
    let (a, a2) = (anna(), anna_dm_v2());
    let dm_v1 = a.dm.pubblica_hex();
    let vecchio = annuncio(&a, &[&a.dm], ORA);
    let nuovo_solo_v2 = annuncio(&a, &[&a2.dm], ORA + 100);
    assert_eq!(identita_del_mittente(R, &[vecchio.clone(), nuovo_solo_v2.clone()], &a2.dm.pubblica_hex()), Some(a.identita.pubblica_hex()));
    assert_eq!(identita_del_mittente(R, &[vecchio.clone(), nuovo_solo_v2.clone()], &dm_v1), None, "chiave ruotata e tolta dall'annuncio");
    // l'ordine in cui arrivano non conta
    assert_eq!(identita_del_mittente(R, &[nuovo_solo_v2, vecchio.clone()], &dm_v1), None);
    // se la nuova ELENCA ancora la v1 (periodo di transizione) la v1 resta valida
    let nuovo_con_entrambe = annuncio(&a, &[&a2.dm, &a.dm], ORA + 100);
    assert_eq!(identita_del_mittente(R, &[vecchio, nuovo_con_entrambe], &dm_v1), Some(a.identita.pubblica_hex()));
}

#[test]
fn annuncio_di_un_altra_rete_o_non_valido_ignorato() {
    let a = anna();
    let dm = dm_di_anna_come_mittente();
    let altra_rete = serde_json::to_value(annuncio_dm(Rete::Principale, &a.identita, &[&a.dm], &[], ORA).unwrap()).unwrap();
    assert_eq!(identita_del_mittente(R, &[altra_rete], &dm), None);
    let mut guasto = annuncio(&a, &[&a.dm], ORA);
    guasto["sig"] = json!("0".repeat(128));
    assert_eq!(identita_del_mittente(R, &[guasto, json!({"non": "un evento"})], &dm), None);
    // un annuncio di kind sbagliato non conta, anche se ben firmato
    let tags: Vec<Vec<String>> = vec![vec!["dm".into(), a.dm.pubblica_hex()]];
    let post = serde_json::to_value(firma_evento(1, "ciao", &tags, ORA, R, &a.identita.pubblica_hex(), |d| a.identita.firma_schnorr(d)).unwrap()).unwrap();
    assert_eq!(identita_del_mittente(R, &[post], &dm), None);
}

#[test]
fn l_annuncio_si_costruisce_solo_con_il_ruolo_giusto() {
    let a = anna();
    assert!(annuncio_dm(R, &a.dm, &[&a.dm], &[], ORA).is_err(), "firma l'identità, non la chiave DM");
    assert!(annuncio_dm(R, &a.identita, &[&a.identita], &[], ORA).is_err(), "nell'elenco ci vanno chiavi DM");
    assert!(annuncio_dm(R, &a.identita, &[], &[], ORA).is_err(), "almeno una chiave DM");
}
