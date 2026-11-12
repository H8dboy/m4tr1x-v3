//! Forum con un nodo finto (niente rete, niente Tor): discussioni e risposte firmate da identità di prova
//! (semi PUBBLICI di prova). Cosa si prova: discussioni e risposte si riconoscono solo dallo schema giusto,
//! eventi falsi o di altri kind si scartano, una risposta conta solo per la sua discussione, il feed di
//! Signal non mostra il forum, i nomi vengono solo da profili verificati.

use m4tr1x_feed::{leggi, leggi_discussione, leggi_forum, BUDGET};
use m4tr1x_identity::{deriva, seme_da_parole, ChiaveRuolo, Ruolo};
use m4tr1x_protocol::{contenuto_profilo, firma_evento, tag_discussione, tag_risposta, Rete, MASSIMO_TITOLO, TAG_FORUM};
use m4tr1x_transport::prova::{relay_finto_che_ignora_il_filtro, relay_finto_condiviso};
use m4tr1x_transport::{BootstrapInfo, Isolation, OnionV3, Stream, Transport, TransportStatus};
use serde_json::{json, Value};
use std::future::Future;
use std::sync::{Arc, Mutex};

const RETE: Rete = Rete::Prova;
const ORA: u64 = 1_760_000_000;

fn chiave(parola: &str, ultima: &str) -> ChiaveRuolo {
    deriva(&seme_da_parole(&([parola; 23].join(" ") + " " + ultima), "").unwrap(), Ruolo::Identity, 1).unwrap()
}
fn anna() -> ChiaveRuolo {
    chiave("abandon", "art")
}
fn bruno() -> ChiaveRuolo {
    chiave("zoo", "vote")
}

fn firma(k: &ChiaveRuolo, kind: u16, testo: &str, tags: &[Vec<String>], ora: u64) -> Value {
    serde_json::to_value(firma_evento(kind, testo, tags, ora, RETE, &k.pubblica_hex(), |d| k.firma_schnorr(d)).unwrap()).unwrap()
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

fn onion() -> OnionV3 {
    OnionV3::from_pubkey([9u8; 32])
}

/// Un forum di prova: due discussioni, tre risposte (una a ciascuna più una seconda alla prima), un post
/// normale, il profilo di Anna, e la robaccia che un nodo può mandare.
fn forum_di_prova() -> (Vec<Value>, String, String) {
    let (a, b) = (anna(), bruno());
    let d1 = firma(&a, 1, "Come si installa un nodo?", &tag_discussione("Nodo su Oracle").unwrap(), ORA);
    let d2 = firma(&b, 1, "Proposte per la musica", &tag_discussione("Musica").unwrap(), ORA + 10);
    let (id1, id2) = (d1["id"].as_str().unwrap().to_string(), d2["id"].as_str().unwrap().to_string());
    let r1 = firma(&b, 1, "Con lo script deploy/installa-nodo.sh", &tag_risposta(&id1).unwrap(), ORA + 100);
    let r2 = firma(&a, 1, "Grazie, funziona", &tag_risposta(&id1).unwrap(), ORA + 200);
    let r3 = firma(&a, 1, "Io voto per gli album", &tag_risposta(&id2).unwrap(), ORA + 50);
    let post = firma(&a, 1, "Un post normale di Signal", &[], ORA + 5);
    let profilo = firma(&a, 0, &contenuto_profilo("Anna", "prova").unwrap(), &[], ORA);
    // robaccia: una discussione con la firma rotta, una risposta con due tag e, una discussione di un altro kind
    let mut rotta = firma(&b, 1, "FALSA", &tag_discussione("Falsa").unwrap(), ORA + 300);
    rotta["content"] = json!("FALSA, cambiata dopo la firma");
    let doppia = firma(&b, 1, "DOPPIA", &[vec!["t".into(), TAG_FORUM.into()], vec!["e".into(), id1.clone()], vec!["e".into(), id2.clone()]], ORA + 400);
    let altro_kind = firma(&b, 30023, "ALTRO KIND", &tag_discussione("Articolo lungo").unwrap(), ORA + 500);
    (vec![d1, d2, r1, r2, r3, post, profilo, rotta, doppia, altro_kind], id1, id2)
}

#[tokio::test]
async fn il_forum_mostra_le_discussioni_dalla_piu_attiva_con_risposte_e_nomi() {
    let (eventi, id1, id2) = forum_di_prova();
    let nodo = Nodo { eventi: Arc::new(Mutex::new(eventi)), malevolo: false };
    let f = leggi_forum(&nodo, &onion(), RETE, BUDGET, |_| {}).await.unwrap();
    let elenco: Vec<(&str, u32)> = f.discussioni.iter().map(|d| (d.id.as_str(), d.risposte)).collect();
    // d1 ha l'ultima risposta (ORA+200) → prima, anche se d2 è più recente
    assert_eq!(elenco, vec![(id1.as_str(), 2), (id2.as_str(), 1)]);
    assert_eq!(f.discussioni[0].titolo, "Nodo su Oracle");
    assert_eq!(f.discussioni[0].nome.as_deref(), Some("Anna"));
    assert_eq!(f.discussioni[1].nome, None, "Bruno non ha un profilo: nessun nome inventato");
    assert_eq!(f.scartati, 1, "la discussione con la firma rotta si conta come scartata");
    assert!(!format!("{f:?}").contains("FALSA") && !format!("{f:?}").contains("Articolo lungo"));
}

#[tokio::test]
async fn una_discussione_aperta_ha_solo_le_sue_risposte_in_ordine() {
    let (eventi, id1, _) = forum_di_prova();
    let nodo = Nodo { eventi: Arc::new(Mutex::new(eventi)), malevolo: false };
    let d = leggi_discussione(&nodo, &onion(), RETE, &id1, BUDGET, |_| {}).await.unwrap().expect("discussione");
    let testi: Vec<&str> = d.risposte.iter().map(|r| r.testo.as_str()).collect();
    assert_eq!(testi, vec!["Con lo script deploy/installa-nodo.sh", "Grazie, funziona"], "risposte di un'altra discussione o con due tag e");
    assert_eq!(d.discussione.risposte, 2);
    assert_eq!(d.risposte[1].nome.as_deref(), Some("Anna"));
    // una discussione che il nodo non ha
    assert!(leggi_discussione(&nodo, &onion(), RETE, &"0".repeat(64), BUDGET, |_| {}).await.unwrap().is_none());
}

#[tokio::test]
async fn un_nodo_che_manda_di_tutto_non_confonde_forum_e_signal() {
    let (eventi, id1, id2) = forum_di_prova();
    let nodo = Nodo { eventi: Arc::new(Mutex::new(eventi)), malevolo: true };
    let f = leggi_forum(&nodo, &onion(), RETE, BUDGET, |_| {}).await.unwrap();
    let mut ids: Vec<&str> = f.discussioni.iter().map(|d| d.id.as_str()).collect();
    ids.sort_unstable();
    let mut attesi = vec![id1.as_str(), id2.as_str()];
    attesi.sort_unstable();
    assert_eq!(ids, attesi, "risposte, post normali o altri kind presi per discussioni");
    let feed = leggi(&nodo, &onion(), RETE, BUDGET, |_| {}).await.unwrap();
    let testi: Vec<&str> = feed.post.iter().map(|p| p.testo.as_str()).collect();
    assert_eq!(testi, vec!["Un post normale di Signal"], "il forum è finito nel feed di Signal");
}

#[tokio::test]
async fn una_risposta_non_si_apre_come_discussione() {
    let (eventi, id1, _) = forum_di_prova();
    let id_risposta = eventi.iter().find(|e| e["content"] == "Grazie, funziona").unwrap()["id"].as_str().unwrap().to_string();
    let nodo = Nodo { eventi: Arc::new(Mutex::new(eventi)), malevolo: true };
    assert!(leggi_discussione(&nodo, &onion(), RETE, &id_risposta, BUDGET, |_| {}).await.unwrap().is_none());
    assert!(leggi_discussione(&nodo, &onion(), RETE, &id1, BUDGET, |_| {}).await.unwrap().is_some());
}

#[test]
fn titoli_e_risposte_si_scrivono_solo_nella_forma_giusta() {
    assert!(tag_discussione("").is_err());
    assert!(tag_discussione("   \u{202E}\u{200B}  ").is_err(), "un titolo fatto solo di caratteri invisibili");
    assert!(tag_discussione(&"x".repeat(MASSIMO_TITOLO + 1)).is_err());
    let t = tag_discussione("  Nodo\u{202E} su\n  Oracle ").unwrap();
    assert_eq!(t[1], vec!["subject".to_string(), "Nodo su Oracle".to_string()], "titolo non ripulito");
    assert!(tag_risposta("non-un-id").is_err());
    assert!(tag_risposta(&"A".repeat(64)).is_err(), "id maiuscolo");
    assert!(tag_risposta(&"a".repeat(64)).is_ok());
}
