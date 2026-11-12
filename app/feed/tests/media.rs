//! Reels, film, musica con un nodo finto: solo schede (kind 1063) verificate, con UN hash valido, un tipo ammesso
//! e adatto alla sezione, una dimensione entro il massimo, una sola sezione. Il resto si scarta, anche se il nodo
//! manda di tutto.

use m4tr1x_feed::{leggi_media, BUDGET};
use m4tr1x_identity::{deriva, seme_da_parole, ChiaveRuolo, Ruolo};
use m4tr1x_protocol::{file_da_evento, firma_evento, tag_autorizzazione_file, tag_file, Rete, Sezione, KIND_FILE, MASSIMO_FILE};
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
    let buono_reel = firma(&a, KIND_FILE, "il mio primo reel", &tag_file(Sezione::Signal, &sha('a'), "video/mp4", 1000, "Primo reel").unwrap(), ORA + 10);
    let buona_foto = firma(&a, KIND_FILE, "una foto", &tag_file(Sezione::Signal, &sha('b'), "image/jpeg", 500, "").unwrap(), ORA + 5);
    let buon_brano = firma(&a, KIND_FILE, "un brano", &tag_file(Sezione::Musica, &sha('c'), "audio/mpeg", 3000, "Hajde").unwrap(), ORA);
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
    assert!(tag_file(Sezione::Signal, "corto", "video/mp4", 10, "").is_err());
    assert!(tag_file(Sezione::Signal, &sha('a'), "image/svg+xml", 10, "").is_err());
    assert!(tag_file(Sezione::Film, &sha('a'), "image/jpeg", 10, "").is_err(), "una foto tra i film");
    assert!(tag_file(Sezione::Musica, &sha('a'), "video/mp4", 10, "").is_err(), "un video nella musica");
    assert!(tag_file(Sezione::Signal, &sha('a'), "video/mp4", 0, "").is_err());
    assert!(tag_file(Sezione::Signal, &sha('a'), "video/mp4", MASSIMO_FILE + 1, "").is_err());
    let t = tag_file(Sezione::Film, &sha('a'), "video/mp4", 10, "  Il\u{202E} film  ").unwrap();
    assert!(t.contains(&s(&["title", "Il film"])), "titolo non ripulito: {t:?}");
    assert!(tag_autorizzazione_file(&sha('a'), ORA, ORA).is_err());
    assert!(tag_autorizzazione_file(&sha('a'), ORA, ORA + 3601).is_err(), "autorizzazione più lunga di un'ora");
    assert!(tag_autorizzazione_file("x", ORA, ORA + 60).is_err());
    // l'url di una scheda esterna non conta: il file si cerca solo per hash sul nodo (la scheda non ha campi url)
    let a = anna();
    let tags = [tag_file(Sezione::Signal, &sha('a'), "video/mp4", 10, "").unwrap(), vec![s(&["url", "https://esempio.invalid/x.mp4"])]].concat();
    let e = m4tr1x_protocol::verifica(&firma(&a, KIND_FILE, "", &tags, ORA), RETE).unwrap();
    let f = file_da_evento(&e).expect("scheda valida anche con un url");
    assert_eq!(f.sha256, sha('a'));
}
