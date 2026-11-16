//! Eventi M4TR1X nel lato Rust dell'app (F4): stessi esiti di `packages/protocol` (JavaScript).
//!
//! Un evento è un evento Nostr (NIP-01) con in più il tag `["net", <rete>]` (I-08): id =
//! SHA-256 di `[0, pubkey, created_at, kind, tags, content]` serializzato come `JSON.stringify`,
//! firma BIP-340 (Schnorr secp256k1) dell'id.
//!
//! Il contratto con il JavaScript sono i vettori CONGELATI `packages/protocol/vectors/eventi.json`
//! (test `tests/vettori.rs`): stesso esito su ogni evento. Qui si è più severi del JavaScript,
//! mai più permissivi: `created_at` intero tra 0 e 2^53−1 scritto senza decimali, nessun
//! surrogato spaiato (Rust non li rappresenta).
//!
//! Dati H8 vietati negli eventi (I-15): stessa regola di `h8Violation`, scritta a mano senza la
//! libreria `regex`: il confine di parola `\b` del JavaScript conosce solo le lettere ASCII,
//! quello di `regex` anche le altre, e su "àm4a1…" i due darebbero esiti diversi.

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Reti ammesse (stesse di `NETWORKS` in packages/protocol).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rete {
    Principale,
    Prova,
}

impl Rete {
    pub fn nome(self) -> &'static str {
        match self {
            Rete::Principale => "m4tr1x-main",
            Rete::Prova => "m4tr1x-test",
        }
    }
}

/// Più grande intero che `JSON.stringify` scrive esatto (Number.MAX_SAFE_INTEGER).
const MAX_INTERO_JS: u64 = (1 << 53) - 1;

/// Evento già verificato: l'unico modo di averne uno è [`verifica`] (o [`firma_evento`], che riverifica).
/// I campi si LEGGONO da fuori ma l'evento non si COSTRUISCE da fuori: il campo privato `_verificato` lo
/// impedisce (20/10/2026: prima, con i campi tutti pubblici, chiunque poteva crearne uno non verificato).
///
/// ```compile_fail
/// let _ = m4tr1x_protocol::Evento { id: String::new(), pubkey: String::new(), created_at: 0, kind: 1,
///     tags: vec![], content: String::new(), sig: String::new() };
/// ```
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Evento {
    pub id: String,
    pub pubkey: String,
    pub created_at: u64,
    pub kind: u16,
    pub tags: Vec<Vec<String>>,
    pub content: String,
    pub sig: String,
    #[serde(skip)]
    _verificato: (),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Rifiuto {
    #[error("forma non valida: {0}")]
    Forma(&'static str),
    #[error("rete sbagliata o tag net mancante/doppio")]
    Rete,
    #[error("dati H8 nell'evento: {0}")]
    H8(&'static str),
    #[error("id non corrispondente al contenuto")]
    Id,
    #[error("firma non valida")]
    Firma,
    #[error("kind {0} vietato sul filo (messaggi privati nudi: solo buste 1059, I-19)")]
    KindVietato(u16),
}

/// Kind dei messaggi privati "nudi": DM vecchio (NIP-04), sigillo (NIP-59) e messaggio (NIP-17). Non
/// viaggiano mai su un relay: i DM escono solo dentro la busta kind 1059 (I-19).
pub const KIND_VIETATI: [u16; 3] = [4, 13, 14];

/// Verifica completa di un evento ricevuto (forma, rete, dati H8, id, firma), come `verifyEvent`.
pub fn verifica(v: &Value, rete: Rete) -> Result<Evento, Rifiuto> {
    let o = v.as_object().ok_or(Rifiuto::Forma("non è un oggetto"))?;
    let testo = |k: &'static str| o.get(k).and_then(Value::as_str).ok_or(Rifiuto::Forma(k));
    let id = testo("id")?;
    let pubkey = testo("pubkey")?;
    let sig = testo("sig")?;
    if !esadecimale_minuscolo(id, 64) || !esadecimale_minuscolo(pubkey, 64) || !esadecimale_minuscolo(sig, 128) {
        return Err(Rifiuto::Forma("id, pubkey o sig"));
    }
    let created_at = intero(o.get("created_at")).filter(|&n| n <= MAX_INTERO_JS).ok_or(Rifiuto::Forma("created_at"))?;
    let kind = intero(o.get("kind")).and_then(|n| u16::try_from(n).ok()).ok_or(Rifiuto::Forma("kind"))?;
    if KIND_VIETATI.contains(&kind) {
        return Err(Rifiuto::KindVietato(kind));
    }
    let content = testo("content")?;
    let tags = o
        .get("tags")
        .and_then(Value::as_array)
        .ok_or(Rifiuto::Forma("tags"))?
        .iter()
        .map(|t| {
            let t = t.as_array().filter(|t| !t.is_empty()).ok_or(Rifiuto::Forma("tag vuoto o non lista"))?;
            t.iter().map(|x| x.as_str().map(str::to_owned).ok_or(Rifiuto::Forma("tag con valori non testo"))).collect()
        })
        .collect::<Result<Vec<Vec<String>>, _>>()?;

    let reti: Vec<_> = tags.iter().filter(|t| t[0] == "net").collect();
    if reti.len() != 1 || reti[0].get(1).map(String::as_str) != Some(rete.nome()) {
        return Err(Rifiuto::Rete);
    }
    if let Some(motivo) = violazione_h8(content, &tags) {
        return Err(Rifiuto::H8(motivo));
    }
    if id_evento(pubkey, created_at, kind, &tags, content) != id {
        return Err(Rifiuto::Id);
    }
    verifica_firma(pubkey, id, sig)?;
    Ok(Evento {
        id: id.to_owned(),
        pubkey: pubkey.to_owned(),
        created_at,
        kind,
        tags,
        content: content.to_owned(),
        sig: sig.to_owned(),
        _verificato: (),
    })
}

/// Testo massimo di un post scritto dall'app (caratteri): ben sotto i 64 KiB per evento del nodo anche
/// con caratteri da 4 byte. Scelta del 20/10/2026, si cambia qui.
pub const MASSIMO_TESTO_POST: usize = 2000;

/// Costruisce e FIRMA un evento come `buildEvent` di packages/protocol: aggiunge il tag `net`, rifiuta i
/// dati H8 PRIMA di firmare (I-15: "il costruttore di eventi rifiuta"), calcola l'id e lo firma con
/// `firma` (BIP-340 sul digest dell'id). Il protocollo non vede la chiave: riceve la pubblica e una
/// funzione che firma (la chiave resta nel crate dell'identità). L'evento firmato si RIVERIFICA prima
/// di restituirlo: un evento che non passerebbe da `verifica` non esce mai di qui.
pub fn firma_evento(
    kind: u16,
    content: &str,
    tags: &[Vec<String>],
    created_at: u64,
    rete: Rete,
    pubkey: &str,
    firma: impl FnOnce(&[u8; 32]) -> Option<[u8; 64]>,
) -> Result<Evento, Rifiuto> {
    if KIND_VIETATI.contains(&kind) {
        return Err(Rifiuto::KindVietato(kind));
    }
    if tags.iter().any(|t| t.is_empty()) {
        return Err(Rifiuto::Forma("tag vuoto"));
    }
    if tags.iter().any(|t| t[0] == "net") {
        return Err(Rifiuto::Forma("il tag net lo aggiunge il protocollo"));
    }
    if created_at > MAX_INTERO_JS {
        return Err(Rifiuto::Forma("created_at"));
    }
    if !esadecimale_minuscolo(pubkey, 64) {
        return Err(Rifiuto::Forma("pubkey"));
    }
    let mut tutti: Vec<Vec<String>> = tags.to_vec();
    tutti.push(vec!["net".into(), rete.nome().into()]);
    if let Some(motivo) = violazione_h8(content, &tutti) {
        return Err(Rifiuto::H8(motivo));
    }
    let id = id_evento(pubkey, created_at, kind, &tutti, content);
    let digest: [u8; 32] = da_esadecimale(&id).and_then(|v| v.try_into().ok()).ok_or(Rifiuto::Id)?;
    let sig = esadecimale(&firma(&digest).ok_or(Rifiuto::Firma)?);
    let ev = serde_json::json!({ "id": id, "pubkey": pubkey, "created_at": created_at, "kind": kind, "tags": tutti, "content": content, "sig": sig });
    verifica(&ev, rete)
}

// ─── Profilo (kind 0, F4) ────────────────────────────────────────────────────
// Decisioni del fondatore (20/10/2026): SOLO `name` e `about`, in lettura e in scrittura; ogni altro
// campo si scarta (niente lud16/lud06: pagamenti nel profilo, contro lo spirito di I-15; niente nip05:
// servirebbe contattare un sito in chiaro; niente picture: URL fuori da Tor, l'immagine arriva in F6).

/// Nome massimo (caratteri).
pub const MASSIMO_NOME: usize = 50;
/// Descrizione massima (caratteri).
pub const MASSIMO_DESCRIZIONE: usize = 500;

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct Profilo {
    pub name: String,
    pub about: String,
}

/// Caratteri invisibili o che girano il verso del testo (U+202E: "emocleW" scritto rovesciato si legge "Welcome"): nel NOME servono solo a
/// falsificare l'identità di qualcun altro, e `textContent` non li ferma (è il testo stesso a ingannare).
fn ingannevole(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}' | '\u{061C}')
}

/// Nome pulito: senza caratteri di controllo, invisibili o di direzione, spazi unificati, al massimo
/// MASSIMO_NOME caratteri.
fn pulisci_nome(s: &str) -> String {
    let senza: String = s.chars().filter(|&c| !c.is_control() && !ingannevole(c)).collect();
    senza.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MASSIMO_NOME).collect()
}

/// Descrizione pulita: a capo ammessi, niente altri caratteri di controllo né di direzione, al massimo
/// MASSIMO_DESCRIZIONE caratteri.
fn pulisci_descrizione(s: &str) -> String {
    s.chars().filter(|&c| (c == '\n' || !c.is_control()) && !ingannevole(c)).take(MASSIMO_DESCRIZIONE).collect::<String>().trim().to_owned()
}

/// Profilo da un evento GIÀ verificato (kind 0). Legge solo `name` e `about` (stringhe); tutto il resto si
/// scarta; testi puliti e tagliati ai limiti. None se non è un kind 0 o il contenuto non è un oggetto JSON.
pub fn profilo_da_evento(e: &Evento) -> Option<Profilo> {
    if e.kind != 0 {
        return None;
    }
    let v: Value = serde_json::from_str(&e.content).ok()?;
    let o = v.as_object()?;
    let testo = |k: &str| o.get(k).and_then(Value::as_str).unwrap_or("");
    Some(Profilo { name: pulisci_nome(testo("name")), about: pulisci_descrizione(testo("about")) })
}

/// Contenuto di un kind 0 scritto dall'app: SOLO `name` e `about`, puliti. Errore se il nome è vuoto o i
/// testi superano i limiti (chi scrive deve saperlo, non vedersi tagliare il testo in silenzio).
pub fn contenuto_profilo(name: &str, about: &str) -> Result<String, Rifiuto> {
    if name.chars().count() > MASSIMO_NOME || about.chars().count() > MASSIMO_DESCRIZIONE {
        return Err(Rifiuto::Forma("nome o descrizione oltre il limite"));
    }
    let p = Profilo { name: pulisci_nome(name), about: pulisci_descrizione(about) };
    if p.name.is_empty() {
        return Err(Rifiuto::Forma("nome vuoto"));
    }
    Ok(serde_json::to_string(&p).expect("due stringhe"))
}

// ─── Forum (stesso schema della v2: kind 1 con tag `t` = m4tr1x-forum) ──────────────────────────────────
// Una DISCUSSIONE è un kind 1 con ["t","m4tr1x-forum"] e il titolo in ["subject", …], senza tag `e`.
// Una RISPOSTA è un kind 1 con ["t","m4tr1x-forum"] e UN solo tag ["e", <id della discussione>, "", "root"]
// (NIP-10). Il tag `t` serve a tenere forum e Signal separati: il feed non mostra né discussioni né risposte.

/// Etichetta del forum (la stessa della v2).
pub const TAG_FORUM: &str = "m4tr1x-forum";
/// Titolo massimo di una discussione (caratteri).
pub const MASSIMO_TITOLO: usize = 120;

fn ha_tag_forum(tags: &[Vec<String>]) -> bool {
    tags.iter().any(|t| t[0] == "t" && t.get(1).map(String::as_str) == Some(TAG_FORUM))
}

/// L'evento appartiene al forum (discussione o risposta): il feed di Signal lo salta.
pub fn e_del_forum(e: &Evento) -> bool {
    e.kind == 1 && ha_tag_forum(&e.tags)
}

/// Titolo pulito come un nome (niente controllo, invisibili, direzione; spazi unificati), al massimo MASSIMO_TITOLO.
fn pulisci_titolo(s: &str) -> String {
    let senza: String = s.chars().filter(|&c| !c.is_control() && !ingannevole(c)).collect();
    senza.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MASSIMO_TITOLO).collect()
}

/// Tag di una discussione nuova scritta dall'app. Errore se il titolo è vuoto o troppo lungo (chi scrive lo
/// deve sapere, non vederselo tagliare).
pub fn tag_discussione(titolo: &str) -> Result<Vec<Vec<String>>, Rifiuto> {
    if titolo.chars().count() > MASSIMO_TITOLO {
        return Err(Rifiuto::Forma("titolo oltre il limite"));
    }
    let t = pulisci_titolo(titolo);
    if t.is_empty() {
        return Err(Rifiuto::Forma("titolo vuoto"));
    }
    Ok(vec![vec!["t".into(), TAG_FORUM.into()], vec!["subject".into(), t]])
}

/// Tag di una risposta alla discussione `radice` (id esadecimale di 64 caratteri).
pub fn tag_risposta(radice: &str) -> Result<Vec<Vec<String>>, Rifiuto> {
    if !esadecimale_minuscolo(radice, 64) {
        return Err(Rifiuto::Forma("id della discussione"));
    }
    Ok(vec![vec!["t".into(), TAG_FORUM.into()], vec!["e".into(), radice.into(), String::new(), "root".into()]])
}

/// Titolo di una discussione, da un evento GIÀ verificato: Some solo se è del forum e non ha tag `e`
/// (altrimenti è una risposta o altro). Senza `subject` il titolo è vuoto (le discussioni della v2 potevano non averlo).
pub fn discussione_da_evento(e: &Evento) -> Option<String> {
    if !e_del_forum(e) || e.tags.iter().any(|t| t[0] == "e") {
        return None;
    }
    let sub = e.tags.iter().find(|t| t[0] == "subject").and_then(|t| t.get(1)).map(String::as_str).unwrap_or("");
    Some(pulisci_titolo(sub))
}

/// La discussione a cui risponde un evento GIÀ verificato: Some(id) solo se è del forum e ha UN solo tag `e`,
/// con id valido. Più tag `e` = struttura che l'app non scrive: non la si attribuisce a nessuna discussione.
pub fn risposta_a(e: &Evento) -> Option<String> {
    if !e_del_forum(e) {
        return None;
    }
    let es: Vec<&Vec<String>> = e.tags.iter().filter(|t| t[0] == "e").collect();
    match es.as_slice() {
        [t] => t.get(1).filter(|id| esadecimale_minuscolo(id, 64)).cloned(),
        _ => None,
    }
}

// ─── File: reels, film, musica (stesso schema della v2: kind 1063 NIP-94 con tag `t` della sezione) ──────
// La scheda dice SOLO: hash del file (`x`), tipo (`m`, da un elenco chiuso), dimensione, sezione, titolo. Il file
// sta sul nodo per hash; chi lo scarica ricontrolla l'hash (I-17) e il tipo dai primi byte.

/// Kind della scheda di un file (NIP-94).
pub const KIND_FILE: u16 = 1063;
/// Kind dell'autorizzazione a caricare un file (BUD-02).
pub const KIND_AUTORIZZAZIONE_FILE: u16 = 24242;
/// Un pezzo di file (ADR-0005): ogni blob del nodo è al massimo così grande. Un file più grande si carica a pezzi,
/// ognuno un blob, e la scheda li elenca in ordine (tag `pezzo`). Su Tor un caricamento lungo cade spesso: a pezzi
/// si riprende da dove si era arrivati, e un nodo piccolo non riceve mai un corpo più grande di questo.
pub const PEZZO: u64 = 4 * 1024 * 1024;
/// File più grande ammesso in una scheda (256 pezzi). Limite dichiarato: il file passa intero in memoria sul
/// computer (pulizia e riproduzione); i film lunghi in streaming aspettano gli hub HLS.
pub const MASSIMO_FILE: u64 = 1024 * 1024 * 1024;

/// Quanti pezzi elenca la scheda di un file di `dimensione` byte: nessuno se sta in un blob solo.
pub fn quanti_pezzi(dimensione: u64) -> usize {
    if dimensione <= PEZZO {
        0
    } else {
        usize::try_from(dimensione.div_ceil(PEZZO)).unwrap_or(usize::MAX)
    }
}

/// I pezzi di una scheda sono nella forma giusta per un file di `dimensione` byte: tanti quanti ne servono
/// (nessuno se il file sta in un blob solo), ognuno un hash valido.
pub fn pezzi_validi(dimensione: u64, pezzi: &[String]) -> bool {
    pezzi.len() == quanti_pezzi(dimensione) && pezzi.iter().all(|p| esadecimale_minuscolo(p, 64))
}
/// Tipi ammessi: gli stessi che la pulizia dei metadati sa trattare (app/media).
pub const MIME_AMMESSI: [&str; 5] = ["image/jpeg", "image/png", "video/mp4", "audio/mp4", "audio/mpeg"];

// ─── File a celle (alveare, docs/design/VIDEO_A_CELLE.md) ────────────────────
// Una foto o un reel codificati dall'app in celle esagonali (crate app/celle) viaggiano come pezzi di MISURA
// DIVERSA: il primo è l'anteprima (pochi KB, l'immagine intera), i seguenti la affinano. La scheda elenca ogni
// pezzo con hash E dimensione (`["pezzo", hash, byte]`), così chi guarda mostra il primo appena arriva e sa quanto
// pesa il resto. Nessun metadato da ripulire: il formato è nostro e porta solo pixel.

/// Una foto a celle (pezzo 0 `M4CF`).
pub const MIME_CELLE_FOTO: &str = "image/vnd.m4tr1x.celle";
/// Un reel a celle (ogni pezzo un gruppo di fotogrammi `M4RL`).
pub const MIME_CELLE_REEL: &str = "video/vnd.m4tr1x.celle";
/// I tipi a celle: pezzi di misura diversa elencati con la dimensione.
pub const MIME_CELLE: [&str; 2] = [MIME_CELLE_FOTO, MIME_CELLE_REEL];
/// Quanti pezzi può elencare una scheda a celle (reel lungo: un pezzo per gruppo di fotogrammi).
pub const MASSIMO_PEZZI_CELLE: usize = 1024;

/// I pezzi di una scheda a celle sono nella forma giusta: almeno uno, non più del massimo, hash validi, misure
/// tutte tra 1 e `PEZZO` byte che sommate danno la dimensione del file.
pub fn strati_validi(dimensione: u64, pezzi: &[String], misure: &[u64]) -> bool {
    !pezzi.is_empty()
        && pezzi.len() <= MASSIMO_PEZZI_CELLE
        && pezzi.len() == misure.len()
        && pezzi.iter().all(|p| esadecimale_minuscolo(p, 64))
        && misure.iter().all(|&m| (1..=PEZZO).contains(&m))
        && misure.iter().try_fold(0u64, |s, &m| s.checked_add(m)) == Some(dimensione)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Sezione {
    /// reels e foto
    Signal,
    Film,
    Musica,
}

impl Sezione {
    pub fn tag(self) -> &'static str {
        match self {
            Sezione::Signal => "m4tr1x-signal",
            Sezione::Film => "m4tr1x-film",
            Sezione::Musica => "m4tr1x-music",
        }
    }
    pub fn da_nome(s: &str) -> Option<Self> {
        match s {
            "signal" => Some(Sezione::Signal),
            "film" => Some(Sezione::Film),
            "musica" => Some(Sezione::Musica),
            _ => None,
        }
    }
    /// Che tipi di file ha senso trovare in ogni sezione.
    pub fn ammette(self, mime: &str) -> bool {
        match self {
            Sezione::Signal => matches!(mime, "image/jpeg" | "image/png" | "video/mp4" | MIME_CELLE_FOTO | MIME_CELLE_REEL),
            Sezione::Film => mime == "video/mp4",
            Sezione::Musica => matches!(mime, "audio/mp4" | "audio/mpeg"),
        }
    }
}

/// Un file descritto da una scheda verificata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SchedaFile {
    pub sha256: String,
    pub mime: String,
    pub dimensione: u64,
    pub sezione: Sezione,
    pub titolo: String,
    /// hash dei pezzi in ordine (ADR-0005); vuoto = il file è un blob solo (`sha256`)
    pub pezzi: Vec<String>,
    /// SOLO per i tipi a celle: la dimensione di ogni pezzo, nello stesso ordine; vuoto per gli altri tipi (lì i
    /// pezzi sono tutti da `PEZZO` byte, l'ultimo più corto)
    pub misure: Vec<u64>,
}

impl SchedaFile {
    /// È una foto o un reel a celle (pezzi con la misura).
    pub fn a_celle(&self) -> bool {
        MIME_CELLE.contains(&self.mime.as_str())
    }
}

/// Tag della scheda di un file scritto dall'app. Errore se qualcosa non è nella forma giusta.
/// `sha256` è l'hash del file INTERO; `pezzi` gli hash dei blob in ordine se il file supera un pezzo.
pub fn tag_file(sezione: Sezione, sha256: &str, mime: &str, dimensione: u64, titolo: &str, pezzi: &[String]) -> Result<Vec<Vec<String>>, Rifiuto> {
    if !MIME_AMMESSI.contains(&mime) {
        return Err(Rifiuto::Forma("tipo di file non adatto alla sezione"));
    }
    if !pezzi_validi(dimensione, pezzi) {
        return Err(Rifiuto::Forma("pezzi del file"));
    }
    let misure: Vec<u64> = vec![];
    tag_scheda(sezione, sha256, mime, dimensione, titolo, pezzi, &misure)
}

/// Tag della scheda di una foto o di un reel a celle: `pezzi` e `misure` (hash e byte di ogni pezzo, in ordine,
/// il primo è l'anteprima); `sha256` e `dimensione` sono del file intero (i pezzi uno dopo l'altro).
pub fn tag_file_a_celle(sezione: Sezione, sha256: &str, mime: &str, dimensione: u64, titolo: &str, pezzi: &[String], misure: &[u64]) -> Result<Vec<Vec<String>>, Rifiuto> {
    if !MIME_CELLE.contains(&mime) {
        return Err(Rifiuto::Forma("tipo di file non a celle"));
    }
    if !strati_validi(dimensione, pezzi, misure) {
        return Err(Rifiuto::Forma("pezzi del file a celle"));
    }
    tag_scheda(sezione, sha256, mime, dimensione, titolo, pezzi, misure)
}

fn tag_scheda(sezione: Sezione, sha256: &str, mime: &str, dimensione: u64, titolo: &str, pezzi: &[String], misure: &[u64]) -> Result<Vec<Vec<String>>, Rifiuto> {
    if !esadecimale_minuscolo(sha256, 64) {
        return Err(Rifiuto::Forma("hash del file"));
    }
    if !sezione.ammette(mime) {
        return Err(Rifiuto::Forma("tipo di file non adatto alla sezione"));
    }
    if dimensione == 0 || dimensione > MASSIMO_FILE {
        return Err(Rifiuto::Forma("dimensione del file"));
    }
    if titolo.chars().count() > MASSIMO_TITOLO {
        return Err(Rifiuto::Forma("titolo oltre il limite"));
    }
    let mut t = vec![
        vec!["x".into(), sha256.into()],
        vec!["m".into(), mime.into()],
        vec!["size".into(), dimensione.to_string()],
        vec!["t".into(), sezione.tag().into()],
    ];
    if misure.is_empty() {
        t.extend(pezzi.iter().map(|p| vec!["pezzo".to_string(), p.clone()]));
    } else {
        t.extend(pezzi.iter().zip(misure).map(|(p, m)| vec!["pezzo".to_string(), p.clone(), m.to_string()]));
    }
    let titolo = pulisci_titolo(titolo);
    if !titolo.is_empty() {
        t.push(vec!["title".into(), titolo]);
    }
    Ok(t)
}

/// La scheda di un file da un evento GIÀ verificato: Some solo se è un kind 1063 con UN hash valido, un tipo
/// ammesso e adatto a UNA sezione, una dimensione entro il massimo, e i pezzi giusti per quella dimensione
/// (nessuno fino a un pezzo, altrimenti tanti quanti ne servono, tutti hash validi; per i tipi a celle ogni pezzo
/// con la sua misura, sommate alla dimensione). Tutto il resto (url esterni compresi) si ignora.
pub fn file_da_evento(e: &Evento) -> Option<SchedaFile> {
    if e.kind != KIND_FILE {
        return None;
    }
    let uno = |n: &str| -> Option<&String> {
        let mut v = e.tags.iter().filter(|t| t[0] == n);
        let primo = v.next()?.get(1)?;
        v.next().is_none().then_some(primo)
    };
    let sha256 = uno("x").filter(|x| esadecimale_minuscolo(x, 64))?.clone();
    // il tipo deve essere adatto alla sezione (controllo sotto): `ammette` accetta solo tipi di MIME_AMMESSI
    let mime = uno("m")?.clone();
    let dimensione: u64 = uno("size")?.parse().ok().filter(|&n| n > 0 && n <= MASSIMO_FILE)?;
    let sezioni: Vec<Sezione> = [Sezione::Signal, Sezione::Film, Sezione::Musica]
        .into_iter()
        .filter(|s| e.tags.iter().any(|t| t[0] == "t" && t.get(1).map(String::as_str) == Some(s.tag())))
        .collect();
    let [sezione] = sezioni.as_slice() else { return None };
    if !sezione.ammette(&mime) {
        return None;
    }
    let tag_pezzi = || e.tags.iter().filter(|t| t[0] == "pezzo");
    let pezzi: Vec<String> = tag_pezzi().map(|t| t.get(1).cloned().unwrap_or_default()).collect();
    let misure: Vec<u64> = if MIME_CELLE.contains(&mime.as_str()) {
        // ogni pezzo a celle porta la misura: una che manca o non è un numero rende la scheda non valida
        let m: Option<Vec<u64>> = tag_pezzi().map(|t| t.get(2)?.parse().ok()).collect();
        let m = m?;
        if !strati_validi(dimensione, &pezzi, &m) {
            return None;
        }
        m
    } else {
        if !pezzi_validi(dimensione, &pezzi) {
            return None;
        }
        vec![]
    };
    let titolo = uno("title").map(|t| pulisci_titolo(t)).unwrap_or_default();
    Some(SchedaFile { sha256, mime, dimensione, sezione: *sezione, titolo, pezzi, misure })
}

/// Tag dell'autorizzazione a caricare il file `sha256` (BUD-02): vale fino a `scadenza` (al massimo un'ora dopo `ora`,
/// come chiede il nodo).
pub fn tag_autorizzazione_file(sha256: &str, ora: u64, scadenza: u64) -> Result<Vec<Vec<String>>, Rifiuto> {
    if !esadecimale_minuscolo(sha256, 64) {
        return Err(Rifiuto::Forma("hash del file"));
    }
    if scadenza <= ora || scadenza > ora + 3600 {
        return Err(Rifiuto::Forma("scadenza dell'autorizzazione"));
    }
    Ok(vec![vec!["t".into(), "upload".into()], vec!["x".into(), sha256.into()], vec!["expiration".into(), scadenza.to_string()]])
}

/// Id dell'evento in esadecimale: SHA-256 della serializzazione NIP-01.
/// `serde_json` scrive le stringhe come `JSON.stringify` (stessi caratteri scappati, stesso
/// `\u00XX` minuscolo, niente `\/`, U+2028 lasciato com'è): lo provano i vettori.
pub fn id_evento(pubkey: &str, created_at: u64, kind: u16, tags: &[Vec<String>], content: &str) -> String {
    let testo = serde_json::to_string(&(0u8, pubkey, created_at, kind, tags, content)).expect("serializzazione di tipi semplici");
    esadecimale(&Sha256::digest(testo.as_bytes()))
}

/// Firma BIP-340 di un id (esadecimale) già calcolato: per chi costruisce eventi fuori da `verifica`
/// (il sigillo dei DM, che non porta il tag `net`).
pub fn verifica_firma(pubkey: &str, id: &str, sig: &str) -> Result<(), Rifiuto> {
    use k256::schnorr::{Signature, VerifyingKey};
    let chiave = VerifyingKey::from_slice(&da_esadecimale(pubkey).ok_or(Rifiuto::Firma)?).map_err(|_| Rifiuto::Firma)?;
    let firma = Signature::try_from(da_esadecimale(sig).ok_or(Rifiuto::Firma)?.as_slice()).map_err(|_| Rifiuto::Firma)?;
    chiave.verify_raw(&da_esadecimale(id).ok_or(Rifiuto::Firma)?, &firma).map_err(|_| Rifiuto::Firma)
}

// ─── Dati H8 vietati (I-15) ──────────────────────────────────────────────────

const TAG_VIETATI: [&str; 5] = ["h8", "h8host", "wallet", "account", "h8account"];

/// Come `h8Violation` di packages/protocol: motivo, o None se l'evento è pulito.
pub fn violazione_h8(content: &str, tags: &[Vec<String>]) -> Option<&'static str> {
    for t in tags {
        if TAG_VIETATI.contains(&t[0].to_lowercase().as_str()) {
            return Some("tag vietato (dati H8)");
        }
        if t.iter().any(|v| contiene_h8(v)) {
            return Some("conto o indirizzo H8 in un tag");
        }
    }
    if contiene_h8(content) {
        return Some("conto o indirizzo H8 nel contenuto");
    }
    None
}

/// `/\bm4a1[0-9a-f]{32}\b/i` oppure `/\bH8[0-9a-f]{38}\b/i`, con il `\b` del JavaScript.
fn contiene_h8(s: &str) -> bool {
    trova(s.as_bytes(), b"m4a1", 32) || trova(s.as_bytes(), b"h8", 38)
}

/// Lettera di parola per il `\b` del JavaScript: solo [A-Za-z0-9_]. I byte non ASCII dell'UTF-8
/// non lo sono mai, come i caratteri non ASCII per il JavaScript.
fn di_parola(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn trova(s: &[u8], prefisso: &[u8], cifre: usize) -> bool {
    let n = prefisso.len() + cifre;
    (0..s.len().saturating_sub(n - 1)).any(|i| {
        let pezzo = &s[i..i + n];
        (i == 0 || !di_parola(s[i - 1]))
            && pezzo[..prefisso.len()].eq_ignore_ascii_case(prefisso)
            && pezzo[prefisso.len()..].iter().all(u8::is_ascii_hexdigit)
            && s.get(i + n).is_none_or(|&b| !di_parola(b))
    })
}

// ─── Piccoli aiuti ───────────────────────────────────────────────────────────

/// Intero non negativo scritto senza decimali (1.0 e -1 si rifiutano).
fn intero(v: Option<&Value>) -> Option<u64> {
    v.and_then(Value::as_u64)
}

fn esadecimale_minuscolo(s: &str, lunghezza: usize) -> bool {
    s.len() == lunghezza && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn esadecimale(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn da_esadecimale(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confini_di_parola_come_il_javascript() {
        let h = "0123456789abcdef".repeat(2);
        assert!(contiene_h8(&format!("m4a1{h}")));
        assert!(contiene_h8(&format!("(M4A1{})", h.to_uppercase())));
        assert!(contiene_h8(&format!("àm4a1{h}")), "lettera non ASCII prima: in JS è un confine");
        assert!(contiene_h8(&format!("m4a1{h}é")), "lettera non ASCII dopo: in JS è un confine");
        assert!(!contiene_h8(&format!("xm4a1{h}")));
        assert!(!contiene_h8(&format!("m4a1{h}_")));
        assert!(!contiene_h8(&format!("m4a1{h}0")));
        assert!(!contiene_h8(&format!("m4a1{}", &h[1..])));
        assert!(!contiene_h8("m4a1"));
        assert!(!contiene_h8(""));
    }

    #[test]
    fn created_at_oltre_gli_interi_esatti_del_javascript_si_rifiuta() {
        assert_eq!(intero(Some(&serde_json::json!(1.0))), None);
        assert_eq!(intero(Some(&serde_json::json!(-1))), None);
        assert_eq!(intero(Some(&serde_json::json!(7))), Some(7));
    }
}
