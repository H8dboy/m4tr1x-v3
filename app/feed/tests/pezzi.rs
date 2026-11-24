//! File a pezzi (ADR-0005) con nodi finti: un file più grande di un PEZZO viaggia a fette da 4 MB, ognuna un blob.
//! Caricamento: i pezzi già sul nodo si saltano (HEAD), l'autorizzazione si firma solo per quelli che partono, un
//! "no" del nodo ferma tutto. Scarico: pezzo per pezzo dalle fonti in ordine (prima il nodo dei file, poi il nodo),
//! ogni pezzo accettato solo con il suo hash, il file ricomposto solo con hash e dimensione della scheda (I-17):
//! un nodo che sostituisce, accorcia o aggiunge un pezzo non passa.

use m4tr1x_feed::{carica_file_a_pezzi, fette, hash_dei_pezzi, pezzi_attesi, scarica_file_a_pezzi, ErroreCarico, ErroreScarico, Fase};
use m4tr1x_protocol::{pezzi_validi, quanti_pezzi, PEZZO};
use m4tr1x_transport::http::sha256_hex;
use m4tr1x_transport::{BootstrapInfo, Isolation, OnionV3, Stream, Transport, TransportStatus};
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// L'archivio dei blob di un nodo finto, condiviso tra le connessioni.
type Blob = Arc<Mutex<HashMap<String, Vec<u8>>>>;

/// Un nodo finto dei file: HEAD/GET/PUT /blob come il nodo vero (hash controllato, `Authorization: Nostr` richiesto).
/// `inganno`: che cosa fa di storto (nessuno, pezzo sostituito, pezzo accorciato, rifiuta tutto).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Inganno {
    Nessuno,
    SostituisceUnPezzo,
    AccorciaUnPezzo,
    RifiutaCaricamenti,
}

struct Nodo {
    blob: Blob,
    inganno: Inganno,
    /// quanti PUT sono arrivati (anche rifiutati)
    put: Arc<Mutex<u32>>,
    /// quante connessioni ha ricevuto (una per richiesta)
    connessioni: Arc<Mutex<u32>>,
}

async fn servi(mut s: tokio::io::DuplexStream, blob: Blob, inganno: Inganno, put: Arc<Mutex<u32>>) {
    let mut testa = Vec::new();
    let mut b = [0u8; 1];
    while !testa.ends_with(b"\r\n\r\n") {
        match s.read(&mut b).await {
            Ok(1) => testa.push(b[0]),
            _ => return,
        }
    }
    let testo = String::from_utf8_lossy(&testa).to_string();
    let prima = testo.lines().next().unwrap_or("").to_string();
    let mut parti = prima.split(' ');
    let (metodo, percorso) = (parti.next().unwrap_or(""), parti.next().unwrap_or(""));
    let sha = percorso.strip_prefix("/blob/").unwrap_or("").to_string();
    let intestazione = |n: &str| testo.lines().find_map(|r| r.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case(n)).map(|(_, v)| v.trim().to_string()));
    let risposta: Vec<u8> = match metodo {
        "HEAD" | "GET" => {
            let trovato = blob.lock().unwrap().get(&sha).cloned();
            match trovato {
                None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
                Some(mut d) => {
                    // l'inganno colpisce il SECONDO pezzo (quello con il primo byte 1, vedi `file_di_prova`)
                    if d.first() == Some(&1) {
                        match inganno {
                            Inganno::SostituisceUnPezzo => d = vec![9u8; d.len()],
                            Inganno::AccorciaUnPezzo => d.truncate(d.len() / 2),
                            _ => {}
                        }
                    }
                    let mut r = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\r\n", d.len()).into_bytes();
                    if metodo == "GET" {
                        r.extend_from_slice(&d);
                    }
                    r
                }
            }
        }
        "PUT" => {
            *put.lock().unwrap() += 1;
            let n: usize = intestazione("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
            let mut corpo = vec![0u8; n];
            if s.read_exact(&mut corpo).await.is_err() {
                return;
            }
            let aut = intestazione("authorization").unwrap_or_default();
            if inganno == Inganno::RifiutaCaricamenti {
                let no = "{\"error\":\"questa chiave non può caricare su questo nodo\"}";
                format!("HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\n\r\n{no}", no.len()).into_bytes()
            } else if !aut.starts_with("Nostr ") {
                b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n".to_vec()
            } else {
                // l'autorizzazione finta porta l'hash autorizzato dopo "Nostr "
                let autorizzato = aut.trim_start_matches("Nostr ").to_string();
                let sha = sha256_hex(&corpo);
                if sha != autorizzato {
                    b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n".to_vec()
                } else {
                    blob.lock().unwrap().insert(sha, corpo);
                    b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\n\r\n{}".to_vec()
                }
            }
        }
        _ => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
    };
    let _ = s.write_all(&risposta).await;
    let _ = s.shutdown().await;
}

/// Due nodi finti dietro un solo trasporto: l'onion `[1; 32]` è il nodo dei file (hub), `[2; 32]` il nodo.
struct Rete {
    hub: Nodo,
    nodo: Nodo,
}

impl Transport for Rete {
    fn bootstrap(&self) -> impl Future<Output = m4tr1x_transport::Result<BootstrapInfo>> + Send {
        async { Ok(BootstrapInfo { millisecondi: 0, implementazione: "finto" }) }
    }
    fn connect_onion(&self, h: &OnionV3, _p: u16, _i: Isolation) -> impl Future<Output = m4tr1x_transport::Result<Stream>> + Send {
        let n = if h.host() == HUB.host() { &self.hub } else { &self.nodo };
        *n.connessioni.lock().unwrap() += 1;
        let (blob, inganno, put) = (n.blob.clone(), n.inganno, n.put.clone());
        async move {
            let (client, server) = tokio::io::duplex(1 << 23);
            tokio::spawn(servi(server, blob, inganno, put));
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

static HUB: std::sync::LazyLock<OnionV3> = std::sync::LazyLock::new(|| OnionV3::from_pubkey([1; 32]));
static NODO: std::sync::LazyLock<OnionV3> = std::sync::LazyLock::new(|| OnionV3::from_pubkey([2; 32]));

fn nodo(inganno: Inganno) -> Nodo {
    Nodo { blob: Arc::new(Mutex::new(HashMap::new())), inganno, put: Arc::new(Mutex::new(0)), connessioni: Arc::new(Mutex::new(0)) }
}

/// Un file di 2 pezzi e mezzo: ogni pezzo comincia con il suo indice (0, 1, 2), così si riconosce.
fn file_di_prova() -> Vec<u8> {
    let pezzo = PEZZO as usize;
    let mut f = vec![0u8; pezzo * 2 + pezzo / 2];
    f[pezzo] = 1;
    f[pezzo * 2] = 2;
    for (i, b) in f.iter_mut().enumerate().skip(1) {
        if i % pezzo != 0 {
            *b = (i % 251) as u8;
        }
    }
    f
}

/// L'autorizzazione finta: "Nostr <hash del pezzo>", e si conta quante volte è stata chiesta.
fn autorizza(conta: &Mutex<Vec<String>>) -> impl Fn(&str) -> Result<String, String> + '_ {
    move |sha| {
        conta.lock().unwrap().push(sha.to_string());
        Ok(format!("Nostr {sha}"))
    }
}

#[test]
fn fette_hash_e_forme_delle_schede() {
    assert_eq!(quanti_pezzi(1), 0);
    assert_eq!(quanti_pezzi(PEZZO), 0, "un file esattamente di un pezzo sta in un blob");
    assert_eq!(quanti_pezzi(PEZZO + 1), 2);
    assert_eq!(quanti_pezzi(PEZZO * 3), 3);
    let piccolo = vec![7u8; 10];
    assert_eq!(fette(&piccolo).len(), 1);
    assert!(hash_dei_pezzi(&piccolo).is_empty(), "un file piccolo non elenca pezzi: il suo hash È il blob");
    let f = file_di_prova();
    let h = hash_dei_pezzi(&f);
    assert_eq!(h.len(), 3);
    assert_eq!(h, fette(&f).iter().map(|p| sha256_hex(p)).collect::<Vec<_>>());
    assert!(pezzi_validi(f.len() as u64, &h));
    assert!(!pezzi_validi(f.len() as u64, &h[..2]), "un pezzo in meno");
    assert!(!pezzi_validi(f.len() as u64, &[h.clone(), vec![h[0].clone()]].concat()), "un pezzo in più");
    assert!(!pezzi_validi(10, &h[..1]), "pezzi per un file che sta in un blob");
    assert!(!pezzi_validi(f.len() as u64, &[h[0].clone(), h[1].clone(), "Z".repeat(64)]), "hash non esadecimale");
    let attesi = pezzi_attesi(&sha256_hex(&f), f.len() as u64, &h).unwrap();
    assert_eq!(attesi.iter().map(|(_, n)| *n).collect::<Vec<_>>(), vec![PEZZO as usize, PEZZO as usize, PEZZO as usize / 2]);
    assert_eq!(pezzi_attesi("a", 10, &[]).unwrap(), vec![("a".to_string(), 10)]);
    assert!(pezzi_attesi("a", 0, &[]).is_none());
    assert!(pezzi_attesi("a", f.len() as u64, &h[..1]).is_none());
}

#[tokio::test]
async fn si_carica_a_pezzi_e_la_seconda_volta_si_saltano_quelli_che_il_nodo_ha_gia() {
    let rete = Rete { hub: nodo(Inganno::Nessuno), nodo: nodo(Inganno::Nessuno) };
    let f = file_di_prova();
    let chieste = Mutex::new(vec![]);
    let fasi = Mutex::new(vec![]);
    carica_file_a_pezzi(&rete, &HUB, &f, autorizza(&chieste), |x| fasi.lock().unwrap().push(x)).await.unwrap();
    let h = hash_dei_pezzi(&f);
    assert_eq!(*chieste.lock().unwrap(), h, "un'autorizzazione per pezzo, nell'ordine dei pezzi");
    assert_eq!(*rete.hub.put.lock().unwrap(), 3);
    assert_eq!(*rete.nodo.put.lock().unwrap(), 0, "i pezzi vanno SOLO al nodo dei file");
    {
        let blob = rete.hub.blob.lock().unwrap();
        assert_eq!(blob.len(), 3);
        for (p, sha) in fette(&f).iter().zip(&h) {
            assert_eq!(blob.get(sha).unwrap(), p);
        }
    }
    let fasi = fasi.into_inner().unwrap();
    assert!(matches!(fasi.first(), Some(Fase::Pezzo { fatti: 0, totali: 3, verso_il_nodo: true, .. })), "{:?}", fasi.first());
    assert!(matches!(fasi.last(), Some(Fase::Pezzo { fatti: 3, totali: 3, verso_il_nodo: true, .. })), "{:?}", fasi.last());

    // seconda volta: tutto già lì → nessuna autorizzazione firmata, nessun PUT (ripresa di un caricamento interrotto)
    chieste.lock().unwrap().clear();
    carica_file_a_pezzi(&rete, &HUB, &f, autorizza(&chieste), |_| {}).await.unwrap();
    assert!(chieste.lock().unwrap().is_empty(), "si sono firmate autorizzazioni per pezzi già sul nodo");
    assert_eq!(*rete.hub.put.lock().unwrap(), 3);

    // a metà: manca solo il secondo pezzo → si firma e si manda solo quello
    rete.hub.blob.lock().unwrap().remove(&h[1]);
    carica_file_a_pezzi(&rete, &HUB, &f, autorizza(&chieste), |_| {}).await.unwrap();
    assert_eq!(*chieste.lock().unwrap(), vec![h[1].clone()]);
    assert_eq!(*rete.hub.put.lock().unwrap(), 4);

    // un file piccolo: un blob solo, con l'hash del file
    let piccolo = vec![3u8; 1000];
    chieste.lock().unwrap().clear();
    carica_file_a_pezzi(&rete, &HUB, &piccolo, autorizza(&chieste), |_| {}).await.unwrap();
    assert_eq!(*chieste.lock().unwrap(), vec![sha256_hex(&piccolo)]);
}

#[tokio::test]
async fn un_no_del_nodo_ferma_il_caricamento_senza_ritentare() {
    let rete = Rete { hub: nodo(Inganno::RifiutaCaricamenti), nodo: nodo(Inganno::Nessuno) };
    let f = file_di_prova();
    let chieste = Mutex::new(vec![]);
    let e = carica_file_a_pezzi(&rete, &HUB, &f, autorizza(&chieste), |_| {}).await.unwrap_err();
    assert!(matches!(e, ErroreCarico::Rifiutato { stato: 403, ref motivo } if motivo.contains("non può caricare")), "{e:?}");
    assert_eq!(*rete.hub.put.lock().unwrap(), 1, "dopo il primo no non si manda altro");
    assert_eq!(chieste.lock().unwrap().len(), 1);
    // un'autorizzazione che non si firma: niente parte
    let e = carica_file_a_pezzi(&Rete { hub: nodo(Inganno::Nessuno), nodo: nodo(Inganno::Nessuno) }, &HUB, &f, |_| Err("portachiavi chiuso".to_string()), |_| {}).await.unwrap_err();
    assert_eq!(e, ErroreCarico::Autorizzazione("portachiavi chiuso".into()));
}

#[tokio::test]
async fn si_scarica_a_pezzi_dalle_fonti_in_ordine_e_si_ricompone_solo_il_file_giusto() {
    let f = file_di_prova();
    let h = hash_dei_pezzi(&f);
    let sha = sha256_hex(&f);
    let dim = f.len() as u64;
    let chieste = Mutex::new(vec![]);

    // il nodo dei file ha tutto: si scarica da lì, il nodo non si tocca
    let rete = Rete { hub: nodo(Inganno::Nessuno), nodo: nodo(Inganno::Nessuno) };
    carica_file_a_pezzi(&rete, &HUB, &f, autorizza(&chieste), |_| {}).await.unwrap();
    let fasi = Mutex::new(vec![]);
    let b = scarica_file_a_pezzi(&rete, &[HUB.clone(), NODO.clone()], &sha, dim, &h, |x| fasi.lock().unwrap().push(x)).await.unwrap();
    assert_eq!(b, f);
    assert!(matches!(fasi.lock().unwrap().last(), Some(Fase::Pezzo { fatti: 3, totali: 3, verso_il_nodo: false, .. })));

    // il nodo dei file NON ha il secondo pezzo: quello si prende dal nodo (l'altra fonte), il resto dal primo
    rete.hub.blob.lock().unwrap().remove(&h[1]);
    rete.nodo.blob.lock().unwrap().insert(h[1].clone(), fette(&f)[1].to_vec());
    assert_eq!(scarica_file_a_pezzi(&rete, &[HUB.clone(), NODO.clone()], &sha, dim, &h, |_| {}).await.unwrap(), f);

    // nessuna fonte ha il pezzo: errore che dice quale
    rete.nodo.blob.lock().unwrap().remove(&h[1]);
    let e = scarica_file_a_pezzi(&rete, &[HUB.clone(), NODO.clone()], &sha, dim, &h, |_| {}).await.unwrap_err();
    assert!(matches!(e, ErroreScarico::NessunaFonte { pezzo: 2, .. }), "{e:?}");

    // una scheda con i pezzi sbagliati non fa partire niente
    assert_eq!(scarica_file_a_pezzi(&rete, &[HUB.clone()], &sha, dim, &h[..1], |_| {}).await.unwrap_err(), ErroreScarico::Scheda);

    // un file piccolo: il blob è il file
    let piccolo = vec![5u8; 777];
    rete.nodo.blob.lock().unwrap().insert(sha256_hex(&piccolo), piccolo.clone());
    assert_eq!(scarica_file_a_pezzi(&rete, &[HUB.clone(), NODO.clone()], &sha256_hex(&piccolo), 777, &[], |_| {}).await.unwrap(), piccolo);
}

#[tokio::test]
async fn un_nodo_che_sostituisce_o_accorcia_un_pezzo_non_passa() {
    let f = file_di_prova();
    let h = hash_dei_pezzi(&f);
    let sha = sha256_hex(&f);
    let dim = f.len() as u64;
    for inganno in [Inganno::SostituisceUnPezzo, Inganno::AccorciaUnPezzo] {
        let rete = Rete { hub: nodo(inganno), nodo: nodo(Inganno::Nessuno) };
        // i blob si mettono direttamente (il PUT li controllerebbe); l'inganno agisce quando si serve il secondo pezzo
        for (p, s) in fette(&f).iter().zip(&h) {
            rete.hub.blob.lock().unwrap().insert(s.clone(), p.to_vec());
        }
        let t0 = std::time::Instant::now();
        let e = scarica_file_a_pezzi(&rete, &[HUB.clone()], &sha, dim, &h, |_| {}).await.unwrap_err();
        // il pezzo sostituito cade sull'hash del pezzo (il client lo rifiuta e la fonte, che "mente", non ha altro da dare);
        // quello accorciato idem: l'hash del pezzo non torna. E non si ritenta: una bugia non è un guasto di rete
        assert!(matches!(e, ErroreScarico::NessunaFonte { pezzo: 2, ref ultimo } if ultimo.contains("hash diverso")), "{inganno:?}: {e:?}");
        assert!(t0.elapsed() < std::time::Duration::from_secs(30), "{inganno:?}: si è ritentato su una fonte che mente ({:?})", t0.elapsed());
        // con una seconda fonte onesta il file arriva lo stesso: la fonte che mente si lascia perdere
        for (p, s) in fette(&f).iter().zip(&h) {
            rete.nodo.blob.lock().unwrap().insert(s.clone(), p.to_vec());
        }
        assert_eq!(scarica_file_a_pezzi(&rete, &[HUB.clone(), NODO.clone()], &sha, dim, &h, |_| {}).await.unwrap(), f, "{inganno:?}");
    }
    // la fonte che mente si lascia perdere DAVVERO: dopo il pezzo sostituito non le si chiede più niente (prima
    // fonte: pezzo 1 giusto, pezzo 2 falso → fuori; il pezzo 3 si chiede solo all'altra). Sfuggito alla campagna
    // di sabotaggio del 24/11/2026: il file arrivava lo stesso e nessuno contava le richieste
    let rete = Rete { hub: nodo(Inganno::SostituisceUnPezzo), nodo: nodo(Inganno::Nessuno) };
    for (p, s) in fette(&f).iter().zip(&h) {
        rete.hub.blob.lock().unwrap().insert(s.clone(), p.to_vec());
        rete.nodo.blob.lock().unwrap().insert(s.clone(), p.to_vec());
    }
    assert_eq!(scarica_file_a_pezzi(&rete, &[HUB.clone(), NODO.clone()], &sha, dim, &h, |_| {}).await.unwrap(), f);
    assert_eq!(*rete.hub.connessioni.lock().unwrap(), 2, "alla fonte che ha mentito si sono chiesti altri pezzi");
    assert_eq!(*rete.nodo.connessioni.lock().unwrap(), 2, "l'altra fonte non ha dato i pezzi 2 e 3");

    // la scheda mente sull'hash intero: i pezzi tornano uno per uno ma il file ricomposto no → scartato
    let rete = Rete { hub: nodo(Inganno::Nessuno), nodo: nodo(Inganno::Nessuno) };
    for (p, s) in fette(&f).iter().zip(&h) {
        rete.hub.blob.lock().unwrap().insert(s.clone(), p.to_vec());
    }
    let e = scarica_file_a_pezzi(&rete, &[HUB.clone()], &"0".repeat(64), dim, &h, |_| {}).await.unwrap_err();
    assert_eq!(e, ErroreScarico::NonCorrisponde);
}

impl std::fmt::Debug for Inganno {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Inganno::Nessuno => "nessuno",
            Inganno::SostituisceUnPezzo => "sostituisce un pezzo",
            Inganno::AccorciaUnPezzo => "accorcia un pezzo",
            Inganno::RifiutaCaricamenti => "rifiuta i caricamenti",
        })
    }
}
