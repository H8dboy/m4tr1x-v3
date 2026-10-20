//! Profilo (kind 0) — decisioni del fondatore del 20/10/2026: solo `name` e `about`, ogni altro campo
//! scartato; nome senza caratteri invisibili o di direzione (falsificazione di nomi); limiti 50/500.

use m4tr1x_identity::{deriva, seme_da_parole, ChiaveRuolo, Ruolo};
use m4tr1x_protocol::{contenuto_profilo, firma_evento, profilo_da_evento, Evento, Profilo, Rete, Rifiuto, MASSIMO_DESCRIZIONE, MASSIMO_NOME};

fn chiave() -> ChiaveRuolo {
    let seme = seme_da_parole(&(["abandon"; 23].join(" ") + " art"), "").unwrap();
    deriva(&seme, Ruolo::Identity, 1).unwrap()
}

fn kind0(content: &str) -> Evento {
    let k = chiave();
    firma_evento(0, content, &[], 1_760_000_000, Rete::Prova, &k.pubblica_hex(), |d| k.firma_schnorr(d)).unwrap()
}

#[test]
fn solo_name_e_about_il_resto_si_scarta() {
    let e = kind0(r#"{"name":"Arta","about":"ciao","lud16":"paga@esempio","nip05":"a@esempio","picture":"https://esempio/a.png","website":"x"}"#);
    let p = profilo_da_evento(&e).unwrap();
    assert_eq!(p, Profilo { name: "Arta".into(), about: "ciao".into() });
    // anche serializzato verso l'interfaccia: solo i due campi
    assert_eq!(serde_json::to_string(&p).unwrap(), r#"{"name":"Arta","about":"ciao"}"#);
}

#[test]
fn nomi_falsificati_ripuliti() {
    // U+202E gira il testo; zero-width e controlli spariscono; spazi unificati
    // il controllo (BEL) arriva SCAPPATO nel JSON (\u0007), come da un nodo vero: crudo non sarebbe JSON valido
    let e = kind0("{\"name\":\"\u{202E}elcomeW \u{200B}Ar\\u0007ta\u{2066}   x\",\"about\":\"riga 1\\nriga 2\u{202D}\"}");
    let p = profilo_da_evento(&e).unwrap();
    assert_eq!(p.name, "elcomeW Arta x");
    assert_eq!(p.about, "riga 1\nriga 2");
}

#[test]
fn testi_lunghi_tagliati_in_lettura() {
    let e = kind0(&serde_json::json!({ "name": "n".repeat(500), "about": "a".repeat(5000) }).to_string());
    let p = profilo_da_evento(&e).unwrap();
    assert_eq!(p.name.chars().count(), MASSIMO_NOME);
    assert_eq!(p.about.chars().count(), MASSIMO_DESCRIZIONE);
}

#[test]
fn contenuti_non_validi_ignorati() {
    for c in ["", "non json", "[1,2]", "\"testo\"", "42"] {
        assert_eq!(profilo_da_evento(&kind0(c)), None, "{c:?}");
    }
    // campi non stringa: vuoti
    assert_eq!(profilo_da_evento(&kind0(r#"{"name":5,"about":{"x":1}}"#)).unwrap(), Profilo::default());
    // non kind 0
    let k = chiave();
    let post = firma_evento(1, r#"{"name":"x"}"#, &[], 1, Rete::Prova, &k.pubblica_hex(), |d| k.firma_schnorr(d)).unwrap();
    assert_eq!(profilo_da_evento(&post), None);
}

#[test]
fn scrittura_solo_due_campi_e_limiti() {
    assert_eq!(contenuto_profilo("  Arta  Prova ", " ciao\n").unwrap(), r#"{"name":"Arta Prova","about":"ciao"}"#);
    assert!(matches!(contenuto_profilo("\u{202E}\u{200B}", "x"), Err(Rifiuto::Forma(_))), "nome fatto solo di invisibili");
    assert!(matches!(contenuto_profilo(&"n".repeat(MASSIMO_NOME + 1), ""), Err(Rifiuto::Forma(_))));
    assert!(matches!(contenuto_profilo("Arta", &"a".repeat(MASSIMO_DESCRIZIONE + 1)), Err(Rifiuto::Forma(_))));
}

#[test]
fn profilo_con_dati_h8_non_si_firma() {
    let k = chiave();
    let c = contenuto_profilo("Arta", &format!("mance a m4a1{}", "0123456789abcdef".repeat(2))).unwrap();
    let e = firma_evento(0, &c, &[], 1, Rete::Prova, &k.pubblica_hex(), |d| k.firma_schnorr(d));
    assert!(matches!(e, Err(Rifiuto::H8(_))), "{e:?}");
}
