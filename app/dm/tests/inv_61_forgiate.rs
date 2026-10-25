//! I-61 — Una busta forgiata o malformata si SCARTA, senza panico e senza mostrare niente: sigillo di un
//! altro, firma del sigillo falsa, messaggio interno firmato, kind sbagliati, versione NIP-44 sconosciuta,
//! MAC alterato, destinatario sbagliato, ora nel futuro, rete sbagliata, contenuto tagliato.

mod comune;
use comune::*;
use m4tr1x_dm::prova::{incarta_difettosa, Difetto};
use m4tr1x_dm::{apri, incarta, EntropiaSistema};
use m4tr1x_protocol::Rete;
use serde_json::{json, Value};

const R: Rete = Rete::Prova;

fn buona() -> (Utente, Utente, Value) {
    let (a, b) = (anna(), bruno());
    let buste = incarta(R, &a.dm, &b.dm.pubblica_hex(), "messaggio buono", ORA, &mut EntropiaSistema).unwrap();
    let v = serde_json::to_value(&buste.per_destinatario).unwrap();
    (a, b, v)
}

#[test]
fn ogni_difetto_e_scartato_e_la_busta_buona_passa() {
    let (a, b, buona) = buona();
    assert!(apri(R, &b.dm, &buona).is_ok(), "la prova parte da una busta buona");
    for d in [
        Difetto::SigilloDiUnAltro,
        Difetto::FirmaDelSigilloFalsa,
        Difetto::MessaggioFirmato,
        Difetto::KindDelMessaggioSbagliato,
        Difetto::KindDelSigilloSbagliato,
        Difetto::VersioneSconosciuta,
        Difetto::MacAlterato,
        Difetto::DestinatarioSbagliato,
        Difetto::OraNelFuturo,
    ] {
        let forgiata = incarta_difettosa(d, R, &a.dm, &b.dm.pubblica_hex(), "messaggio forgiato", ORA, &mut EntropiaSistema).unwrap();
        let r = apri(R, &b.dm, &forgiata);
        assert!(r.is_err(), "{d:?}: la busta forgiata è stata accettata ({r:?})");
    }
}

#[test]
fn rete_sbagliata_contenuto_tagliato_e_forme_strane_scartati_senza_panico() {
    let (_, b, buona) = buona();
    assert!(apri(Rete::Principale, &b.dm, &buona).is_err(), "busta di un'altra rete");
    let mut v = buona.clone();
    let c = v["content"].as_str().unwrap().to_string();
    v["content"] = json!(&c[..c.len() / 2]);
    assert!(apri(R, &b.dm, &v).is_err(), "contenuto tagliato");
    let mut v = buona.clone();
    v["kind"] = json!(1);
    assert!(apri(R, &b.dm, &v).is_err(), "kind diverso da 1059");
    let mut v = buona.clone();
    v["sig"] = json!("0".repeat(128));
    assert!(apri(R, &b.dm, &v).is_err(), "firma della busta falsa");
    for strano in [json!(null), json!(42), json!("x"), json!([]), json!({}), json!({"kind": 1059}), json!({"kind": 1059, "content": 5, "tags": [], "pubkey": "zz", "id": "", "sig": "", "created_at": 1})] {
        assert!(apri(R, &b.dm, &strano).is_err(), "{strano}");
    }
}

#[test]
fn si_apre_solo_con_una_chiave_dm() {
    let (_, b, buona) = buona();
    assert!(apri(R, &b.identita, &buona).is_err(), "l'identità non apre i DM: sono della chiave DM");
}

#[test]
fn dentro_non_ci_sono_chiavi_ne_dati_che_il_messaggio_non_ha_scritto() {
    // il messaggio aperto espone solo mittente (chiave DM), testo e ore: niente segreti, nessun id interno
    let (_, b, buona) = buona();
    let aperto = apri(R, &b.dm, &buona).unwrap();
    let dbg = format!("{aperto:?}");
    assert!(!dbg.contains(&hex(b.dm.segreto_per_portachiavi())));
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
