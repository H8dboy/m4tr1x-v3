//! Rotazione della chiave DM (K1d, 24/11/2026). Cosa si prova qui:
//!   - prima di ruotare la corrente è `dm-v1` (quella del seme);
//!   - ogni rotazione dà una chiave nuova, casuale, che diventa la PRIMA; le precedenti restano fino a `DM_TENUTE`,
//!     quelle più vecchie si cancellano, `dm-v1` resta sempre;
//!   - recuperare la STESSA identità tiene le chiavi ruotate, entrare con un'identità DIVERSA le dimentica;
//!   - una voce `dm-corrente` storta è un errore, non una chiave a caso.

use m4tr1x_identity::creazione::recupera;
use m4tr1x_identity::portachiavi::{chiave_dm_corrente, chiavi_dm, dm_pubbliche, ruota_dm, versione_dm_corrente, Memoria, Portachiavi, DM_TENUTE, VOCE_DM_CORRENTE};

fn parole(a: &str, ultima: &str) -> String {
    [a; 23].join(" ") + " " + ultima
}

fn voci_dm(p: &Memoria) -> Vec<String> {
    let mut v: Vec<String> = p.voci.lock().unwrap().keys().filter(|k| k.starts_with("dm-v")).cloned().collect();
    v.sort();
    v
}

#[test]
fn senza_rotazione_la_corrente_e_quella_del_seme() {
    let p = Memoria::default();
    let id = recupera(&parole("abandon", "art"), &p).unwrap();
    assert_eq!(versione_dm_corrente(&p).unwrap(), 1);
    assert_eq!(dm_pubbliche(&p).unwrap(), vec![id.dm.clone()]);
    assert_eq!(chiave_dm_corrente(&p).unwrap().pubblica_hex(), id.dm);
}

#[test]
fn ogni_rotazione_da_una_chiave_nuova_e_tiene_solo_le_ultime() {
    let p = Memoria::default();
    let id = recupera(&parole("abandon", "art"), &p).unwrap();
    let mut viste = vec![id.dm.clone()];
    for giro in 1..=6u32 {
        let nuova = ruota_dm(&p).unwrap();
        assert!(!viste.contains(&nuova), "la rotazione {giro} ha ridato una chiave già usata");
        viste.push(nuova.clone());
        let pub_ = dm_pubbliche(&p).unwrap();
        assert_eq!(pub_[0], nuova, "la nuova non è la corrente");
        assert_eq!(chiave_dm_corrente(&p).unwrap().pubblica_hex(), nuova);
        assert_eq!(pub_.last().unwrap(), &id.dm, "dm-v1 (dal seme) sparita");
        // ruotate tenute: al massimo DM_TENUTE, più dm-v1
        assert_eq!(pub_.len() as u32, giro.min(DM_TENUTE) + 1, "chiavi tenute dopo {giro} rotazioni");
        assert_eq!(voci_dm(&p).len() as u32, giro.min(DM_TENUTE) + 1, "nel portachiavi restano chiavi uscite dalla finestra");
    }
    // le precedenti tenute sono proprio le ultime, dalla più nuova alla più vecchia
    let pub_ = dm_pubbliche(&p).unwrap();
    assert_eq!(&pub_[..DM_TENUTE as usize], &[viste[6].clone(), viste[5].clone(), viste[4].clone()][..]);
    assert_eq!(chiavi_dm(&p).unwrap().len(), pub_.len());
}

#[test]
fn recupero_della_stessa_identita_tiene_le_ruotate_un_altra_le_dimentica() {
    let p = Memoria::default();
    recupera(&parole("abandon", "art"), &p).unwrap();
    let ruotata = ruota_dm(&p).unwrap();
    // stessa identità sullo stesso dispositivo: la corrente resta quella ruotata
    recupera(&parole("abandon", "art"), &p).unwrap();
    assert_eq!(dm_pubbliche(&p).unwrap()[0], ruotata);
    // identità diversa: niente chiavi ruotate dell'altra, si riparte dalla sua dm-v1
    let bruno = recupera(&parole("zoo", "vote"), &p).unwrap();
    assert_eq!(versione_dm_corrente(&p).unwrap(), 1);
    assert_eq!(dm_pubbliche(&p).unwrap(), vec![bruno.dm]);
    assert_eq!(voci_dm(&p), vec!["dm-v1".to_string()]);
}

#[test]
fn voce_dm_corrente_storta_e_un_errore() {
    let p = Memoria::default();
    recupera(&parole("abandon", "art"), &p).unwrap();
    p.scrivi(VOCE_DM_CORRENTE, &[0u8; 32]).unwrap();
    assert!(chiavi_dm(&p).is_err(), "versione 0 accettata");
    let mut b = [0u8; 32];
    b[0] = 2;
    p.scrivi(VOCE_DM_CORRENTE, &b).unwrap();
    assert!(chiave_dm_corrente(&p).is_err(), "punta a una chiave che non c'è e non dà errore");
}
