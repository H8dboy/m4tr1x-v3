//! I-31 dinamica: i comandi dell'app chiamati dalla VERA IPC di Tauri (runtime finto, capability e
//! configurazione vere dell'app) con un seme creato dall'app stessa. In nessuna risposta, buona o di
//! errore, compaiono byte delle chiavi (identity, dm, revoke, wallet) né 3 parole del seme di fila.
//! Unica eccezione: `seme_nuovo` nella finestra del seme mostra le parole (I-58).
//! Portachiavi in memoria: la prova non tocca mai il portachiavi vero del sistema.

use m4tr1x_identity::portachiavi::{Memoria, Portachiavi};
use m4tr1x_identity::{deriva, seme_da_parole, Ruolo};
use m4tr1x_shell_lib::costruisci;
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Frammento minimo che conta come fuga: 8 byte di fila di una chiave.
const FRAMMENTO: usize = 8;

fn chiama(w: &WebviewWindow<MockRuntime>, cmd: &str, args: Value) -> Result<Value, Value> {
    get_ipc_response(
        w,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: w.url().unwrap(), // origine della finestra stessa: locale su ogni sistema
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|b| b.deserialize::<Value>().unwrap())
}

fn hex_in_byte(s: &str) -> Vec<u8> {
    let v: Vec<u8> = s.bytes().map(|c| (c as char).to_digit(16).unwrap() as u8).collect();
    v.chunks(2).filter(|c| c.len() == 2).map(|c| c[0] << 4 | c[1]).collect()
}

fn base64_in_byte(s: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' | b'-' => 62,
        _ => 63, // '/' '_'
    };
    let v: Vec<u8> = s.bytes().map(val).collect();
    v.chunks(4)
        .filter(|c| c.len() >= 2)
        .flat_map(|c| {
            let n = c.iter().enumerate().fold(0u32, |a, (i, &x)| a | (x as u32) << (18 - 6 * i));
            let b = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
            b[..c.len() - 1].to_vec()
        })
        .collect()
}

/// Solo per l'autocontrollo in fondo alla prova.
fn base64(b: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    b.chunks(3)
        .flat_map(|c| {
            let n = c.iter().enumerate().fold(0u32, |a, (i, &x)| a | (x as u32) << (16 - 8 * i));
            (0..=c.len()).map(move |i| A[(n >> (18 - 6 * i) & 63) as usize] as char)
        })
        .collect()
}

/// Tutte le letture possibili di una risposta: il testo così com'è, ogni sequenza hex (a partire da
/// ogni cifra, così l'allineamento non conta) e ogni sequenza base64 (da ognuno dei 4 inizi).
fn letture(testo: &str) -> Vec<Vec<u8>> {
    let mut out = vec![testo.as_bytes().to_vec()];
    let pezzi = |ok: fn(char) -> bool| -> Vec<String> {
        testo.split(|c: char| !ok(c)).filter(|p| p.len() >= 2 * FRAMMENTO).map(String::from).collect()
    };
    for p in pezzi(|c| c.is_ascii_hexdigit()) {
        out.push(hex_in_byte(&p));
        out.push(hex_in_byte(&p[1..]));
    }
    for p in pezzi(|c| c.is_ascii_alphanumeric() || "+/-_".contains(c)) {
        for i in 0..4.min(p.len()) {
            out.push(base64_in_byte(&p[i..]));
        }
    }
    out
}

fn controlla(cosa: &str, risposta: &Result<Value, Value>, segreti: &[(&str, [u8; 32])], parole: &[String], parole_ammesse: bool) {
    let testo = match risposta {
        Ok(v) | Err(v) => v.to_string(),
    };
    for l in letture(&testo) {
        for (nome, s) in segreti {
            for f in s.windows(FRAMMENTO) {
                assert!(!l.windows(FRAMMENTO).any(|x| x == f), "{cosa}: nella risposta ci sono byte della chiave {nome}");
            }
        }
    }
    if !parole_ammesse {
        let minuscolo = testo.to_lowercase();
        for tre in parole.windows(3) {
            assert!(!minuscolo.contains(&tre.join(" ")) && !minuscolo.contains(&format!("\"{}\",\"{}\",\"{}\"", tre[0], tre[1], tre[2])), "{cosa}: nella risposta ci sono parole del seme");
        }
    }
}

#[test]
fn nessuna_chiave_raggiunge_il_javascript() {
    // il tor dell'app e il nodo di prova restano spenti: questa prova non deve aprire niente
    assert!(std::env::var_os("M4TR1X_TOR_BIN").is_none() && std::env::var_os("M4TR1X_NODO_PROVA").is_none(), "togliere M4TR1X_TOR_BIN e M4TR1X_NODO_PROVA per questa prova");
    let memoria = Arc::new(Memoria::default());
    let mut app = costruisci(mock_builder(), memoria.clone() as Arc<dyn Portachiavi>, |_| Ok(()))
        .build(tauri::generate_context!())
        .expect("app di prova");
    // il setup dell'app (stato della rete, finestre di tauri.conf.json) parte qui: senza, i comandi
    // rispondono "state not managed" e la prova non controllerebbe niente (misurato il 21/10/2026)
    // deprecata perché in un ciclo gira a vuoto; qui è UNA chiamata, solo per far partire il setup
    #[allow(deprecated)]
    app.run_iteration(|_, _| {});
    let main = app.get_webview_window("main").expect("finestra principale da tauri.conf.json");
    let seme = WebviewWindowBuilder::new(&app, "seme", WebviewUrl::App("seme.html?modo=crea".into())).build().unwrap();

    // 1. la finestra principale NON può chiamare i comandi del seme (capability + controllo dell'etichetta)
    for (cmd, args) in [("seme_nuovo", json!({"rivedi": false})), ("seme_conferma", json!({"risposte": ["a", "b", "c"]})), ("seme_recupera", json!({"parole": "abandon ".repeat(23) + "art"}))] {
        let r = chiama(&main, cmd, args);
        assert!(matches!(&r, Err(Value::String(e)) if e.starts_with(&format!("{cmd} not allowed on window \"main\""))), "la finestra principale ha chiamato {cmd}: {r:?}");
    }
    assert!(memoria.voci.lock().unwrap().is_empty(), "la finestra principale ha scritto nel portachiavi");

    // 2. creazione vera dalla finestra del seme: 24 parole, 3 posizioni, conferma giusta
    let nuovo = chiama(&seme, "seme_nuovo", json!({"rivedi": false})).expect("seme_nuovo");
    let chiavi_risposta: Vec<&str> = nuovo.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(chiavi_risposta, ["parole", "posizioni"], "seme_nuovo restituisce altro oltre a parole e posizioni");
    let parole: Vec<String> = nuovo["parole"].as_array().unwrap().iter().map(|p| p.as_str().unwrap().to_string()).collect();
    assert_eq!(parole.len(), 24);
    let posizioni: Vec<usize> = nuovo["posizioni"].as_array().unwrap().iter().map(|p| p.as_u64().unwrap() as usize).collect();
    let s = seme_da_parole(&parole.join(" "), "").unwrap();
    let segreti: Vec<(&str, [u8; 32])> = [("identity", Ruolo::Identity), ("dm", Ruolo::Dm), ("revoke", Ruolo::Revoke), ("wallet", Ruolo::Wallet)]
        .into_iter()
        .map(|(n, r)| (n, *deriva(&s, r, 1).unwrap().segreto_per_portachiavi()))
        .collect();
    controlla("seme_nuovo", &Ok(nuovo.clone()), &segreti, &parole, true);

    let sbagliate = json!({"risposte": ["zoo", "zoo", "zoo"]});
    let r = chiama(&seme, "seme_conferma", sbagliate);
    assert!(r.is_err(), "conferma con parole sbagliate accettata");
    controlla("seme_conferma (sbagliata)", &r, &segreti, &parole, false);
    let giuste: Vec<&str> = posizioni.iter().map(|&p| parole[p - 1].as_str()).collect();
    let r = chiama(&seme, "seme_conferma", json!({"risposte": giuste}));
    assert!(r.is_ok(), "conferma con le parole giuste rifiutata: {r:?}");
    controlla("seme_conferma", &r, &segreti, &parole, false);
    // nel portachiavi ci sono proprio le chiavi ricavate qui: la prova cerca i segreti veri
    let salvate: Vec<[u8; 32]> = memoria.voci.lock().unwrap().values().copied().collect();
    assert_eq!(salvate.len(), 3, "attese identity, dm, wallet");
    for v in &salvate {
        assert!(segreti.iter().any(|(_, s)| s == v), "nel portachiavi c'è una chiave che la prova non conosce");
    }

    // 3. recupero dalle stesse parole (seconda finestra del seme)
    let seme2 = app.get_webview_window("seme").unwrap_or_else(|| WebviewWindowBuilder::new(&app, "seme", WebviewUrl::App("seme.html?modo=recupera".into())).build().unwrap());
    let r = chiama(&seme2, "seme_recupera", json!({"parole": parole.join(" ")}));
    assert!(r.is_ok(), "recupero rifiutato: {r:?}");
    controlla("seme_recupera", &r, &segreti, &parole, false);

    // 4. ogni comando della finestra principale, con l'identità salvata
    // esito atteso di ognuno: un comando che non arriva alla sua funzione (permesso, stato mancante)
    // fa fallire la prova invece di passare senza aver controllato niente. Tor spento: chi usa la
    // rete si ferma a "tor non configurato".
    const OK: &str = "";
    let comandi = [
        ("stato", json!({}), OK),
        ("identita", json!({}), OK),
        ("feed", json!({}), "tor non configurato"),
        ("feed_avanzamento", json!({}), OK),
        ("pubblica", json!({"testo": "prova"}), "tor non configurato"),
        ("pubblica", json!({"testo": ""}), "il post è vuoto"),
        ("pubblica_avanzamento", json!({}), OK),
        ("profilo_mio", json!({}), "tor non configurato"),
        ("profilo_salva", json!({"nome": "prova", "descrizione": "prova"}), "tor non configurato"),
        ("profilo_avanzamento", json!({}), OK),
        ("messaggi", json!({}), "tor non configurato"),
        ("messaggio_invia", json!({"a": "0".repeat(64), "testo": "prova"}), "tor non configurato"),
        ("messaggio_invia", json!({"a": "0".repeat(64), "testo": "  "}), "il messaggio è vuoto"),
        ("messaggi_avanzamento", json!({}), OK),
        ("nodo_impostato", json!({}), OK),
        ("forum", json!({}), "tor non configurato"),
        ("forum_discussione", json!({"id": "a".repeat(64)}), "tor non configurato"),
        ("forum_discussione", json!({"id": "non-un-id"}), "id della discussione non valido"),
        ("forum_pubblica", json!({"titolo": "prova", "testo": "prova"}), "tor non configurato"),
        ("forum_pubblica", json!({"titolo": "", "testo": "prova"}), "titolo vuoto"),
        ("forum_rispondi", json!({"id": "a".repeat(64), "testo": "prova"}), "tor non configurato"),
        ("forum_rispondi", json!({"id": "a".repeat(64), "testo": " "}), "il testo è vuoto"),
        ("forum_avanzamento", json!({}), OK),
        ("media", json!({"sezione": "signal"}), "tor non configurato"),
        ("media", json!({"sezione": "../x"}), "sezione sconosciuta"),
        ("media_file", json!({"sha256": "a".repeat(64), "mime": "video/mp4", "dimensione": 10, "pezzi": []}), "tor non configurato"),
        ("media_file", json!({"sha256": "a".repeat(64), "mime": "text/html", "dimensione": 10, "pezzi": []}), "scheda del file non valida"),
        // pezzi che non tornano con la dimensione: un pezzo per un file che sta in un blob, nessuno per uno grande
        ("media_file", json!({"sha256": "a".repeat(64), "mime": "video/mp4", "dimensione": 10, "pezzi": ["b".repeat(64)]}), "scheda del file non valida"),
        ("media_file", json!({"sha256": "a".repeat(64), "mime": "video/mp4", "dimensione": 5 * 1024 * 1024, "pezzi": []}), "scheda del file non valida"),
        ("media_file", json!({"sha256": "a".repeat(64), "mime": "video/mp4", "dimensione": 5 * 1024 * 1024, "pezzi": ["b".repeat(64), "c".repeat(64)]}), "tor non configurato"),
        ("nodo_file_impostato", json!({}), OK),
        ("nodo_file_imposta", json!({"indirizzo": "esempio.com"}), "indirizzo non valido"),
        ("nodo_file_imposta", json!({"indirizzo": ""}), OK),
        ("media_prepara", json!({}), "serve il contenuto del file"),
        ("media_pubblica", json!({"sezione": "signal", "titolo": "", "testo": ""}), "scegli prima un file"),
        ("media_avanzamento", json!({}), OK),
        ("nodo_imposta", json!({"indirizzo": "esempio.com"}), "indirizzo non valido"),
        ("nodo_imposta", json!({"indirizzo": "192.0.2.1"}), "indirizzo non valido"),
        ("apri_finestra_seme", json!({"modo": "sconosciuto"}), "modo sconosciuto"),
        // I-66: l'elenco dei bloccati (vuoto) e chiavi non valide rifiutate prima di toccare il disco
        ("bloccati", json!({}), OK),
        ("blocca", json!({"chiave": "../bloccati"}), "chiave non valida"),
        ("blocca", json!({"chiave": "a".repeat(63)}), "chiave non valida"),
        ("sblocca", json!({"chiave": "g".repeat(64)}), "chiave non valida"),
        // I-67: id e categoria controllati prima di firmare
        ("segnala", json!({"id": "a".repeat(64), "categoria": "spam"}), "tor non configurato"),
        ("segnala", json!({"id": "non-un-id", "categoria": "spam"}), "forma non valida: id del contenuto"),
        ("segnala", json!({"id": "a".repeat(64), "categoria": "persona"}), "forma non valida: categoria"),
        ("azioni_avanzamento", json!({}), OK),
        // I-68: senza la parola nessuna cancellazione; senza rete le chiavi NON si toccano (controllo sotto)
        ("cancella", json!({"id": "a".repeat(64)}), "tor non configurato"),
        ("cancella", json!({"id": "non-un-id"}), "forma non valida: id del contenuto"),
        ("account_cancella", json!({"conferma": "cancella"}), "per cancellare l'account scrivi CANCELLA"),
        ("account_cancella", json!({"conferma": "CANCELLA"}), "tor non configurato"),
        ("identita_dimentica", json!({"conferma": "si"}), "per togliere l'identità"),
        ("mi_piace", json!({"id": "a".repeat(64)}), "tor non configurato"),
        ("mi_piace", json!({"id": "x"}), "forma non valida: id del contenuto"),
        ("seguiti", json!({}), OK),
        ("segui", json!({"chiave": "x"}), "chiave non valida"),
        ("smetti_di_seguire", json!({"chiave": "x"}), "chiave non valida"),
        ("notifiche_avanzamento", json!({}), OK),
        // fino = 0: niente da salvare (la prova non scrive nella cartella dati)
        ("notifiche_viste", json!({"fino": 0}), OK),
        ("ricondividi", json!({"id": "a".repeat(64)}), "tor non configurato"),
        ("ricondividi", json!({"id": "x"}), "forma non valida: id del post"),
    ];
    for (cmd, args, atteso) in comandi {
        let r = chiama(&main, cmd, args);
        match (&r, atteso) {
            (Ok(_), OK) => {}
            (Err(Value::String(e)), a) if a != OK && e.starts_with(a) => {}
            _ => panic!("{cmd}: risposta inattesa {r:?} (atteso: {})", if atteso == OK { "Ok" } else { atteso }),
        }
        controlla(cmd, &r, &segreti, &parole, false);
    }
    assert_eq!(chiama(&main, "stato", json!({})).unwrap()["trasporto"], "spento", "tor dell'app acceso durante la prova");
    let id = chiama(&main, "identita", json!({})).expect("identita");
    assert!(id["identity"].as_str().is_some_and(|s| s.len() == 64), "identità pubblica attesa nella risposta: {id}");
    // I-68: la cancellazione dell'account non partita (tor spento) ha lasciato le chiavi dove sono
    assert_eq!(memoria.voci.lock().unwrap().len(), 3, "le chiavi sono sparite senza il sì del nodo");
    // solo dal dispositivo: con la parola giusta il portachiavi si svuota, e l'identità non c'è più
    let r = chiama(&main, "identita_dimentica", json!({"conferma": "CANCELLA"}));
    assert!(r.is_ok(), "identita_dimentica rifiutata: {r:?}");
    controlla("identita_dimentica", &r, &segreti, &parole, false);
    assert!(memoria.voci.lock().unwrap().is_empty(), "nel portachiavi resta qualcosa dopo identita_dimentica");
    assert!(chiama(&main, "identita", json!({})).unwrap().is_null(), "identità ancora presente");

    // la prova vede davvero le fughe: una risposta con un pezzo di chiave viene presa
    let finta = Ok(json!({ "x": segreti[0].1[3..3 + FRAMMENTO].iter().map(|b| format!("{b:02x}")).collect::<String>() }));
    let preso = |finta: Result<Value, Value>| std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| controlla("controllo", &finta, &segreti, &parole, false))).is_err();
    assert!(preso(finta), "il controllo non vede un pezzo di chiave in hex");
    assert!(preso(Ok(json!({ "x": base64(&segreti[2].1) }))), "il controllo non vede una chiave in base64");
    assert!(preso(Ok(json!({ "x": format!("zz{}", base64(&segreti[1].1[5..])) }))), "il controllo non vede un pezzo di chiave in base64 non allineato");
    assert!(preso(Ok(json!({ "x": parole[4..7].join(" ") }))), "il controllo non vede 3 parole del seme");
}
