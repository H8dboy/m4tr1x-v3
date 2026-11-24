//! I-60 — Un messaggio si attribuisce a un'identità SOLO se l'annuncio più recente di quell'identità
//! (evento firmato dall'identità) elenca la chiave DM che l'ha sigillato, con la prova di possesso firmata
//! da quella stessa chiave DM. Altrimenti il mittente resta sconosciuto: mai un nome inventato.

mod comune;
use comune::*;
use m4tr1x_dm::{annuncio_dm, apri, dm_dichiarate, identita_del_mittente, incarta, EntropiaSistema, DURATA_ANNUNCIO, DURATA_ANNUNCIO_MASSIMA, KIND_ANNUNCIO_DM};
use m4tr1x_protocol::{firma_evento, Rete};
use serde_json::{json, Value};

const R: Rete = Rete::Prova;
const RELAY: &str = "relay-di-prova-1";
/// Il momento in cui si leggono gli annunci nelle prove: poco dopo `ORA`, prima di ogni scadenza.
const ADESSO: u64 = ORA + 1000;

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
    let (identita, chiavi) = dm_dichiarate(R, &ev, ADESSO).unwrap();
    assert_eq!(identita, a.identita.pubblica_hex());
    assert_eq!(chiavi, vec![a.dm.pubblica_hex()]);
    // il relay dei DM è dichiarato con un tag relay
    assert!(ev["tags"].as_array().unwrap().iter().any(|t| t[0] == "relay" && t[1] == RELAY));
}

#[test]
fn con_l_annuncio_giusto_il_messaggio_e_di_anna() {
    let a = anna();
    let dm = dm_di_anna_come_mittente();
    assert_eq!(identita_del_mittente(R, &[annuncio(&a, &[&a.dm], ORA)], &dm, ADESSO), Some(a.identita.pubblica_hex()));
}

#[test]
fn senza_annuncio_il_mittente_resta_sconosciuto() {
    let dm = dm_di_anna_come_mittente();
    assert_eq!(identita_del_mittente(R, &[], &dm, ADESSO), None);
    // l'annuncio di un'altra persona non c'entra
    let b = bruno();
    assert_eq!(identita_del_mittente(R, &[annuncio(&b, &[&b.dm], ORA)], &dm, ADESSO), None);
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
    assert!(dm_dichiarate(R, &falso, ADESSO).map(|(_, k)| k.is_empty()).unwrap_or(true), "la prova di Anna non vale per l'identità di Bruno");
    assert_eq!(identita_del_mittente(R, &[falso.clone(), annuncio(&a, &[&a.dm], ORA)], &dm, ADESSO), Some(a.identita.pubblica_hex()));
    assert_eq!(identita_del_mittente(R, &[falso], &dm, ADESSO), None);
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
    assert_eq!(identita_del_mittente(R, &[senza], &dm, ADESSO), None, "tag dm senza prova di possesso");
    // prova con una firma sbagliata (tutta a zero)
    let tags2: Vec<Vec<String>> = tags.iter().map(|t| if t[0] == "dm" { vec!["dm".into(), t[1].clone(), "0".repeat(128)] } else { t.clone() }).collect();
    let alterata = serde_json::to_value(firma_evento(KIND_ANNUNCIO_DM, "", &tags2, ORA, R, &a.identita.pubblica_hex(), |d| a.identita.firma_schnorr(d)).unwrap()).unwrap();
    assert_eq!(identita_del_mittente(R, &[alterata], &dm, ADESSO), None, "prova di possesso falsa");
}

#[test]
fn vale_l_annuncio_piu_recente_della_stessa_identita() {
    // Anna ruota la chiave DM: v1 → v2. L'annuncio nuovo elenca SOLO la v2: la v1 non è più attribuita.
    let (a, a2) = (anna(), anna_dm_v2());
    let dm_v1 = a.dm.pubblica_hex();
    let vecchio = annuncio(&a, &[&a.dm], ORA);
    let nuovo_solo_v2 = annuncio(&a, &[&a2.dm], ORA + 100);
    assert_eq!(identita_del_mittente(R, &[vecchio.clone(), nuovo_solo_v2.clone()], &a2.dm.pubblica_hex(), ADESSO), Some(a.identita.pubblica_hex()));
    assert_eq!(identita_del_mittente(R, &[vecchio.clone(), nuovo_solo_v2.clone()], &dm_v1, ADESSO), None, "chiave ruotata e tolta dall'annuncio");
    // l'ordine in cui arrivano non conta
    assert_eq!(identita_del_mittente(R, &[nuovo_solo_v2, vecchio.clone()], &dm_v1, ADESSO), None);
    // se la nuova ELENCA ancora la v1 (periodo di transizione) la v1 resta valida
    let nuovo_con_entrambe = annuncio(&a, &[&a2.dm, &a.dm], ORA + 100);
    assert_eq!(identita_del_mittente(R, &[vecchio, nuovo_con_entrambe], &dm_v1, ADESSO), Some(a.identita.pubblica_hex()));
}

#[test]
fn annuncio_di_un_altra_rete_o_non_valido_ignorato() {
    let a = anna();
    let dm = dm_di_anna_come_mittente();
    let altra_rete = serde_json::to_value(annuncio_dm(Rete::Principale, &a.identita, &[&a.dm], &[], ORA).unwrap()).unwrap();
    assert_eq!(identita_del_mittente(R, &[altra_rete], &dm, ADESSO), None);
    let mut guasto = annuncio(&a, &[&a.dm], ORA);
    guasto["sig"] = json!("0".repeat(128));
    assert_eq!(identita_del_mittente(R, &[guasto, json!({"non": "un evento"})], &dm, ADESSO), None);
    // un annuncio di kind sbagliato non conta, anche se ben firmato
    let tags: Vec<Vec<String>> = vec![vec!["dm".into(), a.dm.pubblica_hex()]];
    let post = serde_json::to_value(firma_evento(1, "ciao", &tags, ORA, R, &a.identita.pubblica_hex(), |d| a.identita.firma_schnorr(d)).unwrap()).unwrap();
    assert_eq!(identita_del_mittente(R, &[post], &dm, ADESSO), None);
}

#[test]
fn l_annuncio_si_costruisce_solo_con_il_ruolo_giusto() {
    let a = anna();
    assert!(annuncio_dm(R, &a.dm, &[&a.dm], &[], ORA).is_err(), "firma l'identità, non la chiave DM");
    assert!(annuncio_dm(R, &a.identita, &[&a.identita], &[], ORA).is_err(), "nell'elenco ci vanno chiavi DM");
    assert!(annuncio_dm(R, &a.identita, &[], &[], ORA).is_err(), "almeno una chiave DM");
}

#[test]
fn se_due_identita_rivendicano_la_stessa_chiave_dm_nessuno_la_ottiene() {
    // le due prove di possesso sono vere (le firma la stessa chiave DM): resta l'ambiguità, quindi nessun nome
    let (a, b) = (anna(), bruno());
    let dm = dm_di_anna_come_mittente();
    let di_anna = annuncio(&a, &[&a.dm], ORA);
    assert_eq!(identita_del_mittente(R, &[di_anna.clone()], &dm, ADESSO), Some(a.identita.pubblica_hex()), "controllo positivo");
    let di_bruno_con_la_chiave_di_anna = annuncio(&b, &[&a.dm], ORA);
    assert_eq!(identita_del_mittente(R, &[di_anna, di_bruno_con_la_chiave_di_anna], &dm, ADESSO), None);
}

#[test]
fn un_evento_di_un_altro_kind_con_gli_stessi_tag_non_e_un_annuncio() {
    let a = anna();
    let buono = annuncio(&a, &[&a.dm], ORA);
    assert!(dm_dichiarate(R, &buono, ADESSO).is_ok(), "controllo positivo");
    let tags: Vec<Vec<String>> = buono["tags"].as_array().unwrap().iter().filter(|t| t[0] != "net").map(|t| t.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect()).collect();
    let ev = firma_evento(1, "", &tags, ORA, R, &a.identita.pubblica_hex(), |d| a.identita.firma_schnorr(d)).unwrap();
    let finto = serde_json::to_value(&ev).unwrap();
    assert!(dm_dichiarate(R, &finto, ADESSO).is_err(), "un kind 1 non dichiara chiavi DM");
    assert_eq!(identita_del_mittente(R, &[finto], &dm_di_anna_come_mittente(), ADESSO), None);
}

/// Annuncio di Anna firmato a mano con i tag di `buono` ma la scadenza sostituita (`None` = senza scadenza).
fn con_scadenza(a: &Utente, buono: &Value, creato: u64, scadenza: Option<Vec<String>>) -> Value {
    let mut tags: Vec<Vec<String>> = buono["tags"].as_array().unwrap().iter().filter(|t| t[0] != "net" && t[0] != "expiration").map(|t| t.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect()).collect();
    if let Some(e) = scadenza {
        tags.push(e);
    }
    serde_json::to_value(firma_evento(KIND_ANNUNCIO_DM, "", &tags, creato, R, &a.identita.pubblica_hex(), |d| a.identita.firma_schnorr(d)).unwrap()).unwrap()
}

#[test]
fn l_annuncio_scade_e_dopo_non_attribuisce_piu_niente() {
    // limite di I-62: un nodo che ripropone un annuncio vecchio può farlo solo fino alla sua scadenza
    let a = anna();
    let dm = dm_di_anna_come_mittente();
    let ev = annuncio(&a, &[&a.dm], ORA);
    let scade: Vec<&Value> = ev["tags"].as_array().unwrap().iter().filter(|t| t[0] == "expiration").collect();
    assert_eq!(scade.len(), 1, "un tag expiration");
    assert_eq!(scade[0][1], (ORA + DURATA_ANNUNCIO).to_string());
    assert_eq!(identita_del_mittente(R, &[ev.clone()], &dm, ORA + DURATA_ANNUNCIO - 1), Some(a.identita.pubblica_hex()), "valido fino all'ultimo secondo");
    assert_eq!(identita_del_mittente(R, &[ev.clone()], &dm, ORA + DURATA_ANNUNCIO), None, "scaduto");
    assert!(dm_dichiarate(R, &ev, ORA + DURATA_ANNUNCIO).is_err());
}

#[test]
fn senza_scadenza_o_con_scadenza_storta_l_annuncio_non_vale() {
    let a = anna();
    let dm = dm_di_anna_come_mittente();
    let buono = annuncio(&a, &[&a.dm], ORA);
    let e = |v: &str| Some(vec!["expiration".to_string(), v.to_string()]);
    // controllo positivo: rifirmato a mano con una scadenza giusta vale
    assert_eq!(identita_del_mittente(R, &[con_scadenza(&a, &buono, ORA, e(&(ORA + 3600).to_string()))], &dm, ADESSO), Some(a.identita.pubblica_hex()));
    for (perche, scadenza) in [
        ("senza scadenza (annunci di prima)", None),
        ("scadenza oltre la durata massima", e(&(ORA + DURATA_ANNUNCIO_MASSIMA + 1).to_string())),
        ("scadenza prima della data", e(&(ORA - 1).to_string())),
        ("scadenza non numerica", e("domani")),
        ("scadenza con segno", e(&format!("+{}", ORA + 3600))),
    ] {
        let ev = con_scadenza(&a, &buono, ORA, scadenza);
        assert_eq!(identita_del_mittente(R, &[ev], &dm, ADESSO), None, "{perche}");
    }
    // due tag expiration: non vale
    let mut tags: Vec<Vec<String>> = buono["tags"].as_array().unwrap().iter().filter(|t| t[0] != "net").map(|t| t.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect()).collect();
    tags.push(vec!["expiration".into(), (ORA + 7200).to_string()]);
    let doppio = serde_json::to_value(firma_evento(KIND_ANNUNCIO_DM, "", &tags, ORA, R, &a.identita.pubblica_hex(), |d| a.identita.firma_schnorr(d)).unwrap()).unwrap();
    assert_eq!(identita_del_mittente(R, &[doppio], &dm, ADESSO), None, "due scadenze");
}

#[test]
fn scaduto_il_piu_recente_non_torna_buono_uno_piu_vecchio_ancora_valido_ma_ruotato() {
    // Anna ruota v1 → v2; il nodo malevolo tiene solo l'annuncio vecchio (v1): vale fino alla sua scadenza, poi più
    let (a, a2) = (anna(), anna_dm_v2());
    let dm_v1 = a.dm.pubblica_hex();
    let vecchio = annuncio(&a, &[&a.dm], ORA);
    let nuovo = annuncio(&a, &[&a2.dm], ORA + 100);
    assert_eq!(identita_del_mittente(R, &[vecchio.clone(), nuovo], &dm_v1, ADESSO), None, "col nuovo presente la v1 non vale");
    assert_eq!(identita_del_mittente(R, std::slice::from_ref(&vecchio), &dm_v1, ADESSO), Some(a.identita.pubblica_hex()), "riproposta: finestra aperta finché non scade");
    assert_eq!(identita_del_mittente(R, &[vecchio], &dm_v1, ORA + DURATA_ANNUNCIO), None, "finestra chiusa alla scadenza");
}
