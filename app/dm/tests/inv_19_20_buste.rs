//! I-19 (i DM escono solo come gift wrap kind 1059) e I-20 (busta firmata da chiave usa e getta, data
//! sfalsata). Due utenti di prova si scrivono (F5): Anna → Bruno.

mod comune;
use comune::*;
use m4tr1x_dm::{apri, incarta, EntropiaSistema, KIND_BUSTA, SFASAMENTO_MASSIMO};
use m4tr1x_protocol::{verifica, Rete};
use std::collections::HashSet;

const R: Rete = Rete::Prova;

#[test]
fn bruno_legge_cio_che_anna_scrive_e_anna_ha_la_sua_copia() {
    let (a, b) = (anna(), bruno());
    let buste = incarta(R, &a.dm, &b.dm.pubblica_hex(), "Ciao Bruno 🦅 — ci vediamo domani", ORA, &mut EntropiaSistema).unwrap();
    let per_b = apri(R, &b.dm, &serde_json::to_value(&buste.per_destinatario).unwrap()).unwrap();
    assert_eq!(per_b.testo, "Ciao Bruno 🦅 — ci vediamo domani");
    assert_eq!(per_b.dm_mittente, a.dm.pubblica_hex());
    assert_eq!(per_b.created_at, ORA);
    let per_a = apri(R, &a.dm, &serde_json::to_value(&buste.per_mittente).unwrap()).unwrap();
    assert_eq!(per_a.testo, per_b.testo, "la copia del mittente si legge con la SUA chiave");
    // né Anna dalla busta di Bruno né Bruno da quella di Anna: ognuno apre solo la propria
    assert!(apri(R, &a.dm, &serde_json::to_value(&buste.per_destinatario).unwrap()).is_err());
    assert!(apri(R, &b.dm, &serde_json::to_value(&buste.per_mittente).unwrap()).is_err());
}

#[test]
fn i19_escono_solo_buste_1059_e_non_mostrano_niente() {
    let (a, b) = (anna(), bruno());
    for i in 0..20 {
        let testo = format!("segreto numero {i}: il pacco arriva alle 9");
        let buste = incarta(R, &a.dm, &b.dm.pubblica_hex(), &testo, ORA, &mut EntropiaSistema).unwrap();
        for e in [&buste.per_destinatario, &buste.per_mittente] {
            assert_eq!(e.kind, KIND_BUSTA, "mai kind 4, 13 o 14 fuori dall'app");
            assert!(verifica(&serde_json::to_value(e).unwrap(), R).is_ok(), "la busta passa da `verifica` come ogni evento");
            // tag: solo il destinatario (p) e la rete (net); nessun altro metadato
            let nomi: Vec<&str> = e.tags.iter().map(|t| t[0].as_str()).collect();
            assert_eq!(nomi.iter().filter(|n| **n == "p").count(), 1);
            assert!(nomi.iter().all(|n| *n == "p" || *n == "net"), "tag inattesi: {nomi:?}");
            // nulla di leggibile: né il testo, né le chiavi, né le parole del messaggio
            let tutto = serde_json::to_string(e).unwrap();
            for non_deve in [testo.as_str(), "pacco", "segreto", &a.dm.pubblica_hex(), &a.identita.pubblica_hex(), &b.identita.pubblica_hex()] {
                assert!(!tutto.contains(non_deve), "la busta mostra «{non_deve}»");
            }
        }
        // il destinatario che il relay vede è la chiave DM del destinatario (non la sua identità)
        let p = |e: &m4tr1x_protocol::Evento| e.tags.iter().find(|t| t[0] == "p").unwrap()[1].clone();
        assert_eq!(p(&buste.per_destinatario), b.dm.pubblica_hex());
        assert_eq!(p(&buste.per_mittente), a.dm.pubblica_hex());
    }
}

#[test]
fn i20_ogni_busta_ha_una_chiave_usa_e_getta() {
    let (a, b) = (anna(), bruno());
    let noi: HashSet<String> = [a.dm.pubblica_hex(), a.identita.pubblica_hex(), b.dm.pubblica_hex(), b.identita.pubblica_hex()].into();
    let mut viste = HashSet::new();
    for _ in 0..20 {
        let buste = incarta(R, &a.dm, &b.dm.pubblica_hex(), "ciao", ORA, &mut EntropiaSistema).unwrap();
        for e in [&buste.per_destinatario, &buste.per_mittente] {
            assert!(!noi.contains(&e.pubkey), "la busta è firmata da una chiave dell'utente");
            assert!(viste.insert(e.pubkey.clone()), "chiave usa e getta riusata");
        }
    }
    assert_eq!(viste.len(), 40);
}

#[test]
fn i20_la_data_e_sfalsata_indietro_fino_a_due_giorni() {
    let (a, b) = (anna(), bruno());
    let mut date = HashSet::new();
    let mut sigilli = HashSet::new();
    for _ in 0..30 {
        let buste = incarta(R, &a.dm, &b.dm.pubblica_hex(), "ciao", ORA, &mut EntropiaSistema).unwrap();
        for e in [&buste.per_destinatario, &buste.per_mittente] {
            assert!(e.created_at <= ORA && e.created_at >= ORA - SFASAMENTO_MASSIMO, "data fuori dall'intervallo: {}", e.created_at);
            date.insert(e.created_at);
        }
        let aperto = apri(R, &b.dm, &serde_json::to_value(&buste.per_destinatario).unwrap()).unwrap();
        assert!(aperto.sigillo_created_at <= ORA && aperto.sigillo_created_at >= ORA - SFASAMENTO_MASSIMO, "data del sigillo fuori dall'intervallo");
        sigilli.insert(aperto.sigillo_created_at);
        assert_eq!(aperto.created_at, ORA, "il messaggio interno porta l'ora vera (solo il destinatario la vede)");
    }
    assert!(date.len() >= 30, "le date delle buste non sono sfalsate davvero ({} diverse su 60)", date.len());
    assert!(sigilli.len() >= 15, "le date dei sigilli non sono sfalsate davvero");
    assert_eq!(SFASAMENTO_MASSIMO, 172_800);
}

#[test]
fn il_messaggio_e_riempito_e_la_dimensione_non_rivela_la_lunghezza_esatta() {
    let (a, b) = (anna(), bruno());
    let len = |t: &str| serde_json::to_string(&incarta(R, &a.dm, &b.dm.pubblica_hex(), t, ORA, &mut EntropiaSistema).unwrap().per_destinatario).unwrap().len();
    // 33 e 60 caratteri cadono nello stesso scaglione di padding NIP-44 (64): stessa dimensione ±(cifre della data)
    let (l1, l2) = (len(&"a".repeat(33)), len(&"a".repeat(60)));
    assert!(l1.abs_diff(l2) <= 4, "{l1} contro {l2}");
}

#[test]
fn testo_vuoto_o_troppo_lungo_rifiutato_prima_di_cifrare() {
    let (a, b) = (anna(), bruno());
    assert!(incarta(R, &a.dm, &b.dm.pubblica_hex(), "ciao", ORA, &mut EntropiaSistema).is_ok(), "controllo positivo");
    assert!(incarta(R, &a.dm, &b.dm.pubblica_hex(), "", ORA, &mut EntropiaSistema).is_err());
    assert!(incarta(R, &a.dm, &b.dm.pubblica_hex(), &"x".repeat(70_000), ORA, &mut EntropiaSistema).is_err());
}

#[test]
fn si_incarta_solo_con_una_chiave_dm_e_verso_una_chiave_valida() {
    let (a, b) = (anna(), bruno());
    assert!(incarta(R, &a.dm, &b.dm.pubblica_hex(), "ciao", ORA, &mut EntropiaSistema).is_ok(), "controllo positivo");
    // l'identità non spedisce DM: il ruolo giusto è Dm (K1d separata, KEYS.md)
    assert!(incarta(R, &a.identita, &b.dm.pubblica_hex(), "ciao", ORA, &mut EntropiaSistema).is_err());
    assert!(incarta(R, &a.dm, "zz", "ciao", ORA, &mut EntropiaSistema).is_err());
    assert!(incarta(R, &a.dm, &"0".repeat(64), "ciao", ORA, &mut EntropiaSistema).is_err(), "pubblica non sulla curva");
}
