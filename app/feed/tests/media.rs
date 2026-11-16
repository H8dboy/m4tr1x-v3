//! Reels, film, musica con un nodo finto: solo schede (kind 1063) verificate, con UN hash valido, un tipo ammesso
//! e adatto alla sezione, una dimensione entro il massimo, una sola sezione. Il resto si scarta, anche se il nodo
//! manda di tutto.

use m4tr1x_feed::{leggi_media, BUDGET};
use m4tr1x_identity::{deriva, seme_da_parole, ChiaveRuolo, Ruolo};
use m4tr1x_protocol::{file_da_evento, firma_evento, strati_validi, tag_autorizzazione_file, tag_file, tag_file_a_celle, Rete, Sezione, KIND_FILE, MASSIMO_FILE, MIME_CELLE_FOTO, MIME_CELLE_REEL, PEZZO};
use m4tr1x_transport::prova::{relay_finto_che_ignora_il_filtro, relay_finto_condiviso};
use m4tr1x_transport::{BootstrapInfo, Isolation, OnionV3, Stream, Transport, TransportStatus};
use serde_json::{json, Value};
use std::future::Future;
use std::sync::{Arc, Mutex};

const RETE: Rete = Rete::Prova;
const ORA: u64 = 1_760_000_000;

fn anna() -> ChiaveRuolo {
    deriva(&seme_da_parole(&(["abandon"; 23].join(" ") + " art"), "").unwrap(), Ruolo::Identity, 1).unwrap()
}
fn firma(k: &ChiaveRuolo, kind: u16, testo: &str, tags: &[Vec<String>], ora: u64) -> Value {
    serde_json::to_value(firma_evento(kind, testo, tags, ora, RETE, &k.pubblica_hex(), |d| k.firma_schnorr(d)).unwrap()).unwrap()
}
fn sha(c: char) -> String {
    std::iter::repeat_n(c, 64).collect()
}
fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

struct Nodo {
    eventi: Arc<Mutex<Vec<Value>>>,
    malevolo: bool,
}
impl Transport for Nodo {
    fn bootstrap(&self) -> impl Future<Output = m4tr1x_transport::Result<BootstrapInfo>> + Send {
        async { Ok(BootstrapInfo { millisecondi: 0, implementazione: "finto" }) }
    }
    fn connect_onion(&self, _h: &OnionV3, _p: u16, _i: Isolation) -> impl Future<Output = m4tr1x_transport::Result<Stream>> + Send {
        let (eventi, malevolo) = (self.eventi.clone(), self.malevolo);
        async move {
            let (client, server) = tokio::io::duplex(1 << 20);
            if malevolo {
                tokio::spawn(relay_finto_che_ignora_il_filtro(server, eventi));
            } else {
                tokio::spawn(relay_finto_condiviso(server, eventi));
            }
            Ok(Box::pin(client) as Stream)
        }
    }
    fn status(&self) -> TransportStatus {
        TransportStatus::Pronto
    }
    fn shutdown(&self) -> impl Future<Output = ()> + Send {
        async {}
    }
}

fn eventi() -> Vec<Value> {
    let a = anna();
    let buono_reel = firma(&a, KIND_FILE, "il mio primo reel", &tag_file(Sezione::Signal, &sha('a'), "video/mp4", 1000, "Primo reel", &[]).unwrap(), ORA + 10);
    let buona_foto = firma(&a, KIND_FILE, "una foto", &tag_file(Sezione::Signal, &sha('b'), "image/jpeg", 500, "", &[]).unwrap(), ORA + 5);
    let buon_brano = firma(&a, KIND_FILE, "un brano", &tag_file(Sezione::Musica, &sha('c'), "audio/mpeg", 3000, "Hajde", &[]).unwrap(), ORA);
    let svg = firma(&a, KIND_FILE, "SVG", &[s(&["x", &sha('d')]), s(&["m", "image/svg+xml"]), s(&["size", "10"]), s(&["t", "m4tr1x-signal"])], ORA + 20);
    let html = firma(&a, KIND_FILE, "HTML", &[s(&["x", &sha('d')]), s(&["m", "text/html"]), s(&["size", "10"]), s(&["t", "m4tr1x-signal"])], ORA + 20);
    let due_hash = firma(&a, KIND_FILE, "DUE HASH", &[s(&["x", &sha('a')]), s(&["x", &sha('e')]), s(&["m", "video/mp4"]), s(&["size", "10"]), s(&["t", "m4tr1x-signal"])], ORA + 20);
    let troppo = firma(&a, KIND_FILE, "TROPPO", &[s(&["x", &sha('e')]), s(&["m", "video/mp4"]), s(&["size", &(MASSIMO_FILE + 1).to_string()]), s(&["t", "m4tr1x-signal"])], ORA + 20);
    let due_sezioni = firma(&a, KIND_FILE, "DUE SEZIONI", &[s(&["x", &sha('e')]), s(&["m", "video/mp4"]), s(&["size", "10"]), s(&["t", "m4tr1x-signal"]), s(&["t", "m4tr1x-film"])], ORA + 20);
    let audio_in_signal = firma(&a, KIND_FILE, "AUDIO IN SIGNAL", &[s(&["x", &sha('e')]), s(&["m", "audio/mpeg"]), s(&["size", "10"]), s(&["t", "m4tr1x-signal"])], ORA + 20);
    let mut rotta = buono_reel.clone();
    rotta["content"] = json!("ROTTA");
    let post_normale = firma(&a, 1, "POST NORMALE", &[s(&["t", "m4tr1x-signal"])], ORA + 30);
    vec![buono_reel, buona_foto, buon_brano, svg, html, due_hash, troppo, due_sezioni, audio_in_signal, rotta, post_normale]
}

#[tokio::test]
async fn in_ogni_sezione_solo_schede_valide_dalla_piu_recente() {
    for malevolo in [false, true] {
        let nodo = Nodo { eventi: Arc::new(Mutex::new(eventi())), malevolo };
        let g = leggi_media(&nodo, &OnionV3::from_pubkey([3; 32]), RETE, Sezione::Signal, BUDGET, |_| {}).await.unwrap();
        let testi: Vec<&str> = g.media.iter().map(|m| m.testo.as_str()).collect();
        assert_eq!(testi, vec!["il mio primo reel", "una foto"], "nodo malevolo: {malevolo}");
        assert_eq!(g.media[0].file.titolo, "Primo reel");
        assert_eq!(g.media[0].file.mime, "video/mp4");
        assert!(!format!("{g:?}").contains("svg") && !format!("{g:?}").contains("html") && !format!("{g:?}").contains("ROTTA"));
        let m = leggi_media(&nodo, &OnionV3::from_pubkey([3; 32]), RETE, Sezione::Musica, BUDGET, |_| {}).await.unwrap();
        assert_eq!(m.media.iter().map(|x| x.file.titolo.as_str()).collect::<Vec<_>>(), vec!["Hajde"]);
        let f = leggi_media(&nodo, &OnionV3::from_pubkey([3; 32]), RETE, Sezione::Film, BUDGET, |_| {}).await.unwrap();
        assert!(f.media.is_empty(), "nel film: {:?}", f.media);
    }
}

#[test]
fn schede_e_autorizzazioni_si_scrivono_solo_nella_forma_giusta() {
    assert!(tag_file(Sezione::Signal, "corto", "video/mp4", 10, "", &[]).is_err());
    assert!(tag_file(Sezione::Signal, &sha('a'), "image/svg+xml", 10, "", &[]).is_err());
    assert!(tag_file(Sezione::Film, &sha('a'), "image/jpeg", 10, "", &[]).is_err(), "una foto tra i film");
    assert!(tag_file(Sezione::Musica, &sha('a'), "video/mp4", 10, "", &[]).is_err(), "un video nella musica");
    assert!(tag_file(Sezione::Signal, &sha('a'), "video/mp4", 0, "", &[]).is_err());
    assert!(tag_file(Sezione::Signal, &sha('a'), "video/mp4", MASSIMO_FILE + 1, "", &[]).is_err());
    let t = tag_file(Sezione::Film, &sha('a'), "video/mp4", 10, "  Il\u{202E} film  ", &[]).unwrap();
    assert!(t.contains(&s(&["title", "Il film"])), "titolo non ripulito: {t:?}");
    assert!(tag_autorizzazione_file(&sha('a'), ORA, ORA).is_err());
    assert!(tag_autorizzazione_file(&sha('a'), ORA, ORA + 3601).is_err(), "autorizzazione più lunga di un'ora");
    assert!(tag_autorizzazione_file("x", ORA, ORA + 60).is_err());
    // l'url di una scheda esterna non conta: il file si cerca solo per hash sul nodo (la scheda non ha campi url)
    let a = anna();
    let tags = [tag_file(Sezione::Signal, &sha('a'), "video/mp4", 10, "", &[]).unwrap(), vec![s(&["url", "https://esempio.invalid/x.mp4"])]].concat();
    let e = m4tr1x_protocol::verifica(&firma(&a, KIND_FILE, "", &tags, ORA), RETE).unwrap();
    let f = file_da_evento(&e).expect("scheda valida anche con un url");
    assert_eq!(f.sha256, sha('a'));
    assert!(f.pezzi.is_empty());

    // file a pezzi (ADR-0005): i pezzi si scrivono e si leggono solo se tornano con la dimensione
    let grande = m4tr1x_protocol::PEZZO * 2 + 1; // 3 pezzi
    let tre = vec![sha('1'), sha('2'), sha('3')];
    assert!(tag_file(Sezione::Film, &sha('a'), "video/mp4", grande, "", &[]).is_err(), "un file grande senza pezzi");
    assert!(tag_file(Sezione::Film, &sha('a'), "video/mp4", grande, "", &tre[..2]).is_err(), "pezzi in meno");
    assert!(tag_file(Sezione::Film, &sha('a'), "video/mp4", 10, "", &tre[..1]).is_err(), "pezzi per un file che sta in un blob");
    assert!(tag_file(Sezione::Film, &sha('a'), "video/mp4", grande, "", &[sha('1'), sha('2'), "G".repeat(64)]).is_err(), "pezzo con hash non valido");
    let t = tag_file(Sezione::Film, &sha('a'), "video/mp4", grande, "Un film", &tre).unwrap();
    assert_eq!(t.iter().filter(|x| x[0] == "pezzo").map(|x| x[1].clone()).collect::<Vec<_>>(), tre, "pezzi nell'ordine dato");
    let e = m4tr1x_protocol::verifica(&firma(&a, KIND_FILE, "", &t, ORA), RETE).unwrap();
    let f = file_da_evento(&e).expect("scheda a pezzi valida");
    assert_eq!((f.pezzi.clone(), f.dimensione), (tre.clone(), grande));
    // dal nodo: un pezzo in meno, uno in più, uno storpiato, pezzi su un file piccolo → scheda ignorata
    let base = |dim: u64| vec![s(&["x", &sha('a')]), s(&["m", "video/mp4"]), s(&["size", &dim.to_string()]), s(&["t", "m4tr1x-film"])];
    let pezzo = |h: &str| s(&["pezzo", h]);
    for (nome, tags) in [
        ("uno in meno", [base(grande), vec![pezzo(&sha('1')), pezzo(&sha('2'))]].concat()),
        ("uno in più", [base(grande), vec![pezzo(&sha('1')), pezzo(&sha('2')), pezzo(&sha('3')), pezzo(&sha('4'))]].concat()),
        ("storpiato", [base(grande), vec![pezzo(&sha('1')), pezzo(&sha('2')), pezzo("non-un-hash")]].concat()),
        ("senza hash", [base(grande), vec![pezzo(&sha('1')), pezzo(&sha('2')), vec!["pezzo".to_string()]]].concat()),
        ("file piccolo con pezzi", [base(10), vec![pezzo(&sha('1'))]].concat()),
        ("file grande senza pezzi", base(grande)),
    ] {
        let e = m4tr1x_protocol::verifica(&firma(&a, KIND_FILE, "", &tags, ORA), RETE).unwrap();
        assert!(file_da_evento(&e).is_none(), "scheda accettata: {nome}");
    }
}

#[test]
fn le_schede_a_celle_portano_hash_e_misura_di_ogni_pezzo() {
    use m4tr1x_transport::http::sha256_hex;
    let pezzi: Vec<Vec<u8>> = vec![vec![1u8; 300], vec![2u8; 5000], vec![3u8; 70]];
    let hash: Vec<String> = pezzi.iter().map(|p| sha256_hex(p)).collect();
    let misure: Vec<u64> = pezzi.iter().map(|p| p.len() as u64).collect();
    let intero = pezzi.concat();
    let (sha_intero, dim) = (sha256_hex(&intero), intero.len() as u64);
    assert!(strati_validi(dim, &hash, &misure));
    assert!(!strati_validi(dim + 1, &hash, &misure), "misure che non sommano alla dimensione");
    assert!(!strati_validi(dim, &hash, &misure[..2]), "una misura in meno");
    assert!(!strati_validi(dim, &[], &[]), "nessun pezzo");
    assert!(!strati_validi(PEZZO + 1, &hash[..1], &[PEZZO + 1]), "un pezzo oltre il massimo");
    assert!(!strati_validi(dim, &hash, &[0, 5000, 370]), "un pezzo vuoto");
    // la scheda classica non accetta i tipi a celle, quella a celle non accetta gli altri
    assert!(tag_file(Sezione::Signal, &sha_intero, MIME_CELLE_FOTO, dim, "", &[]).is_err());
    assert!(tag_file_a_celle(Sezione::Signal, &sha_intero, "image/jpeg", dim, "", &hash, &misure).is_err());
    assert!(tag_file_a_celle(Sezione::Film, &sha_intero, MIME_CELLE_REEL, dim, "", &hash, &misure).is_err(), "i reel a celle stanno nei reels, non tra i film");
    assert!(tag_file_a_celle(Sezione::Signal, &sha_intero, MIME_CELLE_FOTO, dim, "", &hash, &misure[..2]).is_err());
    let t = tag_file_a_celle(Sezione::Signal, &sha_intero, MIME_CELLE_FOTO, dim, "Una foto", &hash, &misure).unwrap();
    let tag_pezzi: Vec<&Vec<String>> = t.iter().filter(|x| x[0] == "pezzo").collect();
    assert_eq!(tag_pezzi.len(), 3);
    assert_eq!(tag_pezzi[1], &vec!["pezzo".to_string(), hash[1].clone(), "5000".to_string()], "ogni pezzo con hash e misura");
    // dalla scheda tornano pezzi e misure; una misura che manca o storpiata rende la scheda non valida
    let a = anna();
    let scheda = |tags: &[Vec<String>]| m4tr1x_protocol::verifica(&firma(&a, KIND_FILE, "", tags, ORA), RETE).ok().and_then(|e| file_da_evento(&e));
    let f = scheda(&t).expect("scheda a celle valida");
    assert!(f.a_celle());
    assert_eq!((f.pezzi.clone(), f.misure.clone(), f.dimensione), (hash.clone(), misure.clone(), dim));
    let mut senza = t.clone();
    senza[5].truncate(2);
    assert!(scheda(&senza).is_none(), "pezzo a celle senza misura");
    let mut storpia = t.clone();
    storpia[5][2] = "tanti".into();
    assert!(scheda(&storpia).is_none(), "misura non numerica");
    let mut sbagliata = t.clone();
    sbagliata[5][2] = "5001".into();
    assert!(scheda(&sbagliata).is_none(), "misure che non sommano alla dimensione");
    // un tipo classico resta classico: niente misure, pezzi da PEZZO
    let classica = scheda(&tag_file(Sezione::Signal, &sha_intero, "image/jpeg", dim, "", &[]).unwrap()).unwrap();
    assert!(!classica.a_celle() && classica.misure.is_empty());
}
