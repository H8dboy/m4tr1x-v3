//! File del nodo (Blossom, ADR-0005) sopra un flusso onion GIÀ aperto da [`crate::Transport::connect_onion`]:
//! `GET /blob/<sha256>` e `PUT /blob`. HTTP/1.1 minimo scritto a mano, come il WebSocket del relay: qui non si
//! apre nessun socket e non c'è TLS (dentro Tor non serve: l'onion è già autenticato e cifrato).
//!
//! Limiti contro un nodo che mente o non risponde: intestazioni ≤ 16 KiB, solo risposte con `Content-Length`
//! (niente `chunked`), corpo ≤ il massimo chiesto, tempo massimo per tutta l'operazione. Il file scaricato si
//! accetta SOLO se il suo SHA-256 è quello chiesto (I-17): un nodo non può sostituire un file con un altro.

use crate::{OnionV3, Result, Stream, TransportError};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{timeout_at, Instant};

/// Intestazioni della risposta al massimo.
pub const MASSIMO_INTESTAZIONI: usize = 16 * 1024;
/// Corpo di una risposta che non è un file (es. la conferma di un caricamento).
pub const MASSIMO_RISPOSTA: usize = 16 * 1024;

fn errore(e: impl std::fmt::Display) -> TransportError {
    TransportError::Relay(format!("file: {e}"))
}

/// Tempo massimo di un trasferimento di `n` byte via Tor: 60 s più 4 s per ogni MiB (≈ 0,25 MiB/s, la parte
/// bassa delle misure del 15/10/2026), al massimo 30 minuti.
pub fn tempo_per(n: usize) -> Duration {
    Duration::from_secs((60 + 4 * (n as u64 / (1024 * 1024))).min(1800))
}

fn esadecimale(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Hash di un file, come lo chiama il nodo.
pub fn sha256_hex(b: &[u8]) -> String {
    esadecimale(&Sha256::digest(b))
}

fn sha_valido(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

struct Risposta {
    stato: u16,
    corpo: Vec<u8>,
}

/// Legge una risposta HTTP/1.1 con `Content-Length` ≤ `massimo`, entro `scadenza`.
async fn leggi_risposta(f: &mut Stream, massimo: usize, scadenza: Instant) -> Result<Risposta> {
    leggi_risposta_a(f, massimo, scadenza, true).await
}

/// Come sopra; con `con_corpo = false` (risposta a una `HEAD`) si leggono solo le intestazioni: il
/// `Content-Length` dice quanto è grande il file ma il corpo non arriva.
async fn leggi_risposta_a(f: &mut Stream, massimo: usize, scadenza: Instant, con_corpo: bool) -> Result<Risposta> {
    let mut testa = Vec::with_capacity(1024);
    let mut b = [0u8; 1];
    // byte per byte fino a CRLF CRLF: le intestazioni sono poche e corte, e così il corpo non si legge per sbaglio
    while !testa.ends_with(b"\r\n\r\n") {
        if testa.len() >= MASSIMO_INTESTAZIONI {
            return Err(errore("intestazioni troppo lunghe"));
        }
        let n = timeout_at(scadenza, f.read(&mut b)).await.map_err(|_| errore("tempo massimo superato"))?.map_err(errore)?;
        if n == 0 {
            return Err(errore("il nodo ha chiuso la connessione"));
        }
        testa.push(b[0]);
    }
    let testo = std::str::from_utf8(&testa).map_err(|_| errore("intestazioni non leggibili"))?;
    let mut righe = testo.split("\r\n");
    let prima = righe.next().unwrap_or("");
    let mut parti = prima.splitn(3, ' ');
    if !parti.next().is_some_and(|v| v == "HTTP/1.1" || v == "HTTP/1.0") {
        return Err(errore("risposta non HTTP"));
    }
    let stato: u16 = parti.next().and_then(|s| s.parse().ok()).ok_or_else(|| errore("stato della risposta"))?;
    let mut lunghezza: Option<usize> = None;
    for r in righe.filter(|r| !r.is_empty()) {
        let (k, v) = r.split_once(':').ok_or_else(|| errore("intestazione malformata"))?;
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
        if k == "transfer-encoding" {
            return Err(errore("risposta a pezzi (chunked) non ammessa"));
        }
        if k == "content-length" {
            let n: usize = v.parse().map_err(|_| errore("Content-Length non valido"))?;
            if lunghezza.is_some_and(|x| x != n) {
                return Err(errore("due Content-Length diversi"));
            }
            lunghezza = Some(n);
        }
    }
    let n = lunghezza.ok_or_else(|| errore("risposta senza Content-Length"))?;
    if n > massimo {
        return Err(errore(format!("risposta troppo grande ({n} byte, massimo {massimo})")));
    }
    if !con_corpo {
        return Ok(Risposta { stato, corpo: vec![] });
    }
    let mut corpo = vec![0u8; n];
    timeout_at(scadenza, f.read_exact(&mut corpo)).await.map_err(|_| errore("tempo massimo superato"))?.map_err(errore)?;
    Ok(Risposta { stato, corpo })
}

/// Scarica il file `sha256` dal nodo: al massimo `massimo` byte, e SOLO se l'hash torna (I-17).
/// Un nodo che non ce l'ha, o che manda un altro file, è un errore come gli altri; per distinguerli c'è [`scarica_se_ce`].
pub async fn scarica(f: Stream, host: &OnionV3, sha256: &str, massimo: usize) -> Result<Vec<u8>> {
    match scarica_se_ce(f, host, sha256, massimo).await? {
        Scaricato::File(b) => Ok(b),
        Scaricato::NonCeLHa => Err(errore("il nodo non ha questo file")),
        Scaricato::HashDiverso => Err(errore("il file ricevuto non è quello chiesto (hash diverso): scartato")),
    }
}

/// Esito di [`scarica_se_ce`]: il file (con l'hash giusto), oppure una RISPOSTA del nodo che non è un guasto di rete
/// e quindi non si ritenta: non ce l'ha (404), o ha mandato un altro file (mente: si passa a un'altra fonte).
#[derive(Debug, PartialEq, Eq)]
pub enum Scaricato {
    File(Vec<u8>),
    NonCeLHa,
    HashDiverso,
}

/// Come [`scarica`], ma un 404 e un file con l'hash sbagliato tornano come esiti, non come errori: chi scarica a
/// pezzi da più fonti passa alla prossima senza ritentare su questa. Il file si accetta SOLO con l'hash chiesto (I-17).
pub async fn scarica_se_ce(mut f: Stream, host: &OnionV3, sha256: &str, massimo: usize) -> Result<Scaricato> {
    if !sha_valido(sha256) {
        return Err(errore("hash del file non valido"));
    }
    let scadenza = Instant::now() + tempo_per(massimo);
    let richiesta = format!("GET /blob/{sha256} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: application/octet-stream\r\n\r\n", host.host());
    timeout_at(scadenza, f.write_all(richiesta.as_bytes())).await.map_err(|_| errore("tempo massimo superato"))?.map_err(errore)?;
    let r = leggi_risposta(&mut f, massimo, scadenza).await?;
    let _ = f.shutdown().await;
    match r.stato {
        200 => {}
        404 => return Ok(Scaricato::NonCeLHa),
        s => return Err(errore(format!("il nodo ha risposto {s}"))),
    }
    if sha256_hex(&r.corpo) != sha256 {
        return Ok(Scaricato::HashDiverso);
    }
    Ok(Scaricato::File(r.corpo))
}

/// Il nodo ha già il blob `sha256`? (`HEAD /blob/<sha>`: 200 sì, 404 no). Serve a riprendere un caricamento a
/// pezzi da dove si era interrotto senza rimandare quello che c'è già. Non dice niente sul contenuto: chi scarica
/// verifica comunque l'hash.
pub async fn esiste(mut f: Stream, host: &OnionV3, sha256: &str) -> Result<bool> {
    if !sha_valido(sha256) {
        return Err(errore("hash del file non valido"));
    }
    let scadenza = Instant::now() + tempo_per(0);
    let richiesta = format!("HEAD /blob/{sha256} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n", host.host());
    timeout_at(scadenza, f.write_all(richiesta.as_bytes())).await.map_err(|_| errore("tempo massimo superato"))?.map_err(errore)?;
    let r = leggi_risposta_a(&mut f, usize::MAX, scadenza, false).await?;
    let _ = f.shutdown().await;
    match r.stato {
        200 => Ok(true),
        404 => Ok(false),
        s => Err(errore(format!("il nodo ha risposto {s}"))),
    }
}

/// L'intestazione `Authorization` di un caricamento: `Nostr <base64 dell'evento kind 24242 firmato>`.
pub fn autorizzazione(evento_json: &str) -> String {
    format!("Nostr {}", data_encoding::BASE64.encode(evento_json.as_bytes()))
}

/// Esito di un caricamento: il nodo ha accettato (`Ok`) o ha detto no con un motivo.
#[derive(Debug, PartialEq, Eq)]
pub enum Caricato {
    Accettato,
    Rifiutato { stato: u16, motivo: String },
}

/// Carica `corpo` sul nodo con l'autorizzazione già firmata (`Nostr <base64 dell'evento kind 24242>`).
pub async fn carica(mut f: Stream, host: &OnionV3, autorizzazione: &str, corpo: &[u8]) -> Result<Caricato> {
    if !autorizzazione.starts_with("Nostr ") || autorizzazione.contains(['\r', '\n']) {
        return Err(errore("autorizzazione non valida"));
    }
    let scadenza = Instant::now() + tempo_per(corpo.len());
    let testa = format!(
        "PUT /blob HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nAuthorization: {autorizzazione}\r\n\r\n",
        host.host(),
        corpo.len()
    );
    let manda = async {
        f.write_all(testa.as_bytes()).await?;
        f.write_all(corpo).await?;
        f.flush().await
    };
    timeout_at(scadenza, manda).await.map_err(|_| errore("tempo massimo superato"))?.map_err(errore)?;
    let r = leggi_risposta(&mut f, MASSIMO_RISPOSTA, scadenza).await?;
    let _ = f.shutdown().await;
    if r.stato == 201 || r.stato == 200 {
        return Ok(Caricato::Accettato);
    }
    // il motivo arriva dal nodo: solo testo stampabile, corto
    let motivo: String = serde_json::from_slice::<serde_json::Value>(&r.corpo)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_owned))
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect();
    Ok(Caricato::Rifiutato { stato: r.stato, motivo })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    fn host() -> OnionV3 {
        OnionV3::from_pubkey([5u8; 32])
    }

    /// Un nodo finto che legge la richiesta e risponde con `risposta` (byte grezzi).
    fn nodo_finto(risposta: Vec<u8>) -> (Stream, tokio::task::JoinHandle<Vec<u8>>) {
        let (client, mut server) = tokio::io::duplex(1 << 22);
        let h = tokio::spawn(async move {
            let mut ricevuto = vec![0u8; 0];
            let mut b = [0u8; 4096];
            // legge finché trova la fine delle intestazioni (e poi quello che c'è)
            loop {
                let n = tokio::time::timeout(Duration::from_millis(300), server.read(&mut b)).await.ok().and_then(|r| r.ok()).unwrap_or(0);
                if n == 0 {
                    break;
                }
                ricevuto.extend_from_slice(&b[..n]);
                if ricevuto.windows(4).any(|w| w == b"\r\n\r\n") && !ricevuto.starts_with(b"PUT") {
                    break;
                }
            }
            let _ = server.write_all(&risposta).await;
            let _ = server.shutdown().await;
            ricevuto
        });
        (Box::pin(client), h)
    }

    fn ok(corpo: &[u8]) -> Vec<u8> {
        [format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", corpo.len()).as_bytes(), corpo].concat()
    }

    #[tokio::test]
    async fn scarica_solo_il_file_con_l_hash_chiesto() {
        let file = b"contenuto del file".to_vec();
        let sha = sha256_hex(&file);
        let (f, h) = nodo_finto(ok(&file));
        assert_eq!(scarica(f, &host(), &sha, 1024).await.unwrap(), file);
        let richiesta = String::from_utf8(h.await.unwrap()).unwrap();
        assert!(richiesta.starts_with(&format!("GET /blob/{sha} HTTP/1.1\r\nHost: {}\r\n", host().host())), "{richiesta}");
        // un nodo che manda un ALTRO file al posto di quello chiesto
        let (f, _) = nodo_finto(ok(b"un altro file"));
        assert!(scarica(f, &host(), &sha, 1024).await.unwrap_err().to_string().contains("hash diverso"));
    }

    #[tokio::test]
    async fn risposte_strane_si_rifiutano() {
        let sha = sha256_hex(b"x");
        for (risposta, atteso) in [
            (b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1\r\nx\r\n0\r\n\r\n".to_vec(), "chunked"),
            (b"HTTP/1.1 200 OK\r\n\r\nx".to_vec(), "senza Content-Length"),
            (b"HTTP/1.1 200 OK\r\nContent-Length: 99999\r\n\r\n".to_vec(), "troppo grande"),
            (b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nx".to_vec(), "due Content-Length"),
            (b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(), "non ha questo file"),
            (b"SSH-2.0-OpenSSH\r\n\r\n".to_vec(), "non HTTP"),
            ([b"HTTP/1.1 200 OK\r\nX: ".as_slice(), &vec![b'a'; MASSIMO_INTESTAZIONI], b"\r\n\r\n"].concat(), "intestazioni troppo lunghe"),
            (b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nxy".to_vec(), "file"),
        ] {
            let (f, _) = nodo_finto(risposta);
            let e = scarica(f, &host(), &sha, 1024).await.unwrap_err().to_string();
            assert!(e.contains(atteso), "atteso {atteso:?}, avuto {e:?}");
        }
        let (f, _) = nodo_finto(ok(b"x"));
        assert!(scarica(f, &host(), "../../etc/passwd", 1024).await.is_err(), "hash non valido usato in un percorso");
    }

    #[tokio::test(start_paused = true)]
    async fn un_nodo_che_non_risponde_scade() {
        let (client, _server) = tokio::io::duplex(1024);
        let e = scarica(Box::pin(client), &host(), &sha256_hex(b"x"), 1024).await.unwrap_err().to_string();
        assert!(e.contains("tempo massimo"), "{e}");
    }

    #[tokio::test]
    async fn esiste_chiede_con_head_e_legge_solo_le_intestazioni() {
        let sha = sha256_hex(b"pezzo");
        // 200 con Content-Length ma SENZA corpo (è una HEAD): non si aspetta nessun byte
        let (f, h) = nodo_finto(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: application/octet-stream\r\n\r\n".to_vec());
        assert!(esiste(f, &host(), &sha).await.unwrap());
        let richiesta = String::from_utf8(h.await.unwrap()).unwrap();
        assert!(richiesta.starts_with(&format!("HEAD /blob/{sha} HTTP/1.1\r\nHost: {}\r\n", host().host())), "{richiesta}");
        let (f, _) = nodo_finto(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec());
        assert!(!esiste(f, &host(), &sha).await.unwrap());
        let (f, _) = nodo_finto(b"HTTP/1.1 500 Boh\r\nContent-Length: 0\r\n\r\n".to_vec());
        assert!(esiste(f, &host(), &sha).await.is_err());
        let (f, _) = nodo_finto(vec![]);
        assert!(esiste(f, &host(), "../x").await.is_err(), "hash non valido usato in un percorso");
    }

    #[tokio::test]
    async fn scarica_se_ce_distingue_il_404_dagli_errori() {
        let file = b"un pezzo".to_vec();
        let sha = sha256_hex(&file);
        let (f, _) = nodo_finto(ok(&file));
        assert_eq!(scarica_se_ce(f, &host(), &sha, 1024).await.unwrap(), Scaricato::File(file));
        let (f, _) = nodo_finto(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec());
        assert_eq!(scarica_se_ce(f, &host(), &sha, 1024).await.unwrap(), Scaricato::NonCeLHa);
        // un altro file al posto di quello chiesto: il nodo mente, i byte non si consegnano (I-17)
        let (f, _) = nodo_finto(ok(b"altro"));
        assert_eq!(scarica_se_ce(f, &host(), &sha, 1024).await.unwrap(), Scaricato::HashDiverso);
        // un guasto (risposta non HTTP) resta un errore: quello sì si ritenta
        let (f, _) = nodo_finto(b"SSH-2.0-OpenSSH\r\n\r\n".to_vec());
        assert!(scarica_se_ce(f, &host(), &sha, 1024).await.is_err());
    }

    #[tokio::test]
    async fn carica_manda_l_autorizzazione_e_legge_il_motivo_di_un_no() {
        let corpo = b"FILE PULITO".to_vec();
        let (f, h) = nodo_finto(b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\n\r\n{}".to_vec());
        assert_eq!(carica(f, &host(), "Nostr eyJ9", &corpo).await.unwrap(), Caricato::Accettato);
        let inviato = h.await.unwrap();
        let testo = String::from_utf8_lossy(&inviato);
        assert!(testo.starts_with("PUT /blob HTTP/1.1\r\n") && testo.contains("Authorization: Nostr eyJ9\r\n") && testo.contains("Content-Length: 11\r\n"), "{testo}");
        assert!(inviato.ends_with(&corpo));
        let no = b"{\"error\":\"questa chiave non pu\xc3\xb2 caricare\\u0007 su questo nodo\"}";
        let (f, _) = nodo_finto([format!("HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\n\r\n", no.len()).as_bytes(), no].concat());
        match carica(f, &host(), "Nostr eyJ9", &corpo).await.unwrap() {
            Caricato::Rifiutato { stato: 403, motivo } => assert_eq!(motivo, "questa chiave non può caricare su questo nodo"),
            altro => panic!("{altro:?}"),
        }
        // un'autorizzazione con un a capo (iniezione di intestazioni) non parte
        let (f, _) = nodo_finto(vec![]);
        assert!(carica(f, &host(), "Nostr x\r\nX-Altro: y", &corpo).await.is_err());
    }
}
