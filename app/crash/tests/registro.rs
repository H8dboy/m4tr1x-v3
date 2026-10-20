//! Registro dei crash: il panic finisce nel file con il punto del codice, i testi costruiti a runtime
//! (che possono contenere segreti) no. Un test solo: l'hook dei panic è unico per tutto il processo.

use std::panic::catch_unwind;

#[test]
fn panic_registrati_senza_segreti() {
    let dir = std::env::temp_dir().join(format!("m4-crash-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    m4tr1x_crash::installa(dir.clone(), "0.0.0-prova");

    // 1. testo fisso del codice: si scrive intero, con il punto
    let _ = catch_unwind(|| panic!("tor non avviato: prova fissa"));
    // 2. testo costruito a runtime con dati finti "segreti": NON si scrive
    let seme = ["abandon"; 23].join(" ") + " art";
    let onion = "a".repeat(56) + ".onion";
    let _ = catch_unwind(move || panic!("errore con {seme} e {onion} e C:\\Users\\qualcuno"));
    // 3. unwrap su un errore (il caso più comune): messaggio costruito, NON si scrive
    let _ = catch_unwind(|| {
        let r: Result<(), String> = Err("chiave 5f".repeat(8));
        r.unwrap();
    });

    let log = std::fs::read_to_string(dir.join(m4tr1x_crash::NOME_FILE)).expect("crash.log non scritto");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(log.matches("=== crash").count(), 3, "{log}");
    assert!(log.contains("messaggio: tor non avviato: prova fissa"), "{log}");
    assert!(log.contains("registro.rs:"), "manca il punto del codice: {log}");
    assert!(log.contains("M4TR1X 0.0.0-prova"), "{log}");
    assert!(log.contains("omesso perché può contenere dati"), "{log}");
    for segreto in ["abandon", ".onion", "qualcuno", "chiave 5f"] {
        assert!(!log.contains(segreto), "nel registro è finito un dato a runtime: {segreto}\n{log}");
    }
}

#[test]
fn cartella_predefinita_con_l_identificatore() {
    let c = m4tr1x_crash::cartella_predefinita("org.m4tr1x.app").expect("cartella dati dell'utente");
    assert!(c.ends_with("org.m4tr1x.app"), "{}", c.display());
}
