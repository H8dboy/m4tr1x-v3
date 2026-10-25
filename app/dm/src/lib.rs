//! Messaggi privati (F5, ADR-0018): NIP-17 (messaggio kind 14 → sigillo kind 13 → busta kind 1059, NIP-44 v2)
//! con chiave DM separata annunciata dall'identità. Limite dichiarato: nessuna forward secrecy in v3.

use m4tr1x_identity::{ChiaveRuolo, Ruolo};
use m4tr1x_protocol::{firma_evento, id_evento, verifica, verifica_firma, Evento, Rete};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

pub mod nip44;

#[cfg(feature = "prova")]
pub mod prova;

/// Kind dell'annuncio della chiave DM: nostro (NON il 10050 di NIP-17, che è la lista dei relay), sostituibile,
/// firmato dall'identità (ADR-0018).
pub const KIND_ANNUNCIO_DM: u16 = 10477;
/// Kind della busta (NIP-59): l'UNICO kind con cui un DM esce dall'app (I-19).
pub const KIND_BUSTA: u16 = 1059;
/// Kind del sigillo (NIP-59) e del messaggio (NIP-17): vivono SOLO dentro la busta cifrata.
const KIND_SIGILLO: u16 = 13;
const KIND_MESSAGGIO: u16 = 14;
/// La data della busta è sfalsata indietro fino a 2 giorni (I-20).
pub const SFASAMENTO_MASSIMO: u64 = 2 * 24 * 3600;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ErroreDm {
    #[error("chiave non valida")]
    Chiave,
    #[error("messaggio non valido: {0}")]
    Messaggio(&'static str),
    #[error("cifratura: {0}")]
    Cifratura(&'static str),
    #[error("busta scartata: {0}")]
    Scartata(&'static str),
    #[error("evento rifiutato dal protocollo: {0}")]
    Protocollo(String),
}

/// Fonte di casualità: il sistema in produzione, una finta nelle prove.
pub trait Entropia {
    fn riempi(&mut self, dove: &mut [u8]);
}

pub struct EntropiaSistema;
impl Entropia for EntropiaSistema {
    fn riempi(&mut self, dove: &mut [u8]) {
        getrandom::fill(dove).expect("il sistema non dà byte casuali: meglio fermarsi che cifrare male");
    }
}

/// Le due buste di un messaggio: una per il destinatario, una per il mittente (la sua copia).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Buste {
    pub per_destinatario: Evento,
    pub per_mittente: Evento,
}

/// Un messaggio aperto e controllato (forma, firma del sigillo, coerenza sigillo/messaggio).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aperto {
    /// Chiave DM che ha sigillato il messaggio: NON è ancora un'identità (vedi [`identita_del_mittente`]).
    pub dm_mittente: String,
    pub testo: String,
    /// Ora dichiarata nel messaggio.
    pub created_at: u64,
    /// Ora (sfalsata) del sigillo, per le prove.
    pub sigillo_created_at: u64,
}


const MAX_RELAY: usize = 8;
const MAX_LUNGHEZZA_RELAY: usize = 200;
const MAX_CHIAVI_DM: usize = 8;

// ─── Difetti (solo prove) ────────────────────────────────────────────────────

/// Difetto da introdurre in una busta forgiata (le prove di I-61). In produzione è sempre `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "prova"), allow(dead_code))]
pub(crate) enum Guasto {
    SigilloDiUnAltro,
    FirmaDelSigilloFalsa,
    MessaggioFirmato,
    KindDelMessaggioSbagliato,
    KindDelSigilloSbagliato,
    VersioneSconosciuta,
    MacAlterato,
    DestinatarioSbagliato,
    OraNelFuturo,
    IdDelMessaggioSbagliato,
    IdDelSigilloSbagliato,
    SigilloNelFuturo,
    TagPDellaBustaSbagliato,
}

// ─── Utilità ─────────────────────────────────────────────────────────────────

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn da_hex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 || !s.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) {
        return None;
    }
    let mut o = [0u8; 32];
    for (i, b) in o.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(o)
}

fn casuale_32(e: &mut dyn Entropia) -> [u8; 32] {
    let mut b = [0u8; 32];
    e.riempi(&mut b);
    b
}

/// Sfalsamento 0..=2 giorni indietro (I-20).
fn sfalsamento(e: &mut dyn Entropia) -> u64 {
    let mut b = [0u8; 8];
    e.riempi(&mut b);
    u64::from_le_bytes(b) % (SFASAMENTO_MASSIMO + 1)
}

/// Chiave usa e getta per una busta: segreto casuale valido sulla curva.
struct Effimera {
    segreto: [u8; 32],
    pubblica: String,
}
impl Drop for Effimera {
    fn drop(&mut self) {
        self.segreto.zeroize();
    }
}
fn effimera(e: &mut dyn Entropia) -> Result<Effimera, ErroreDm> {
    for _ in 0..16 {
        let segreto = casuale_32(e);
        if let Ok(sk) = k256::schnorr::SigningKey::from_slice(&segreto) {
            return Ok(Effimera { pubblica: hex(&sk.verifying_key().to_bytes()), segreto });
        }
    }
    Err(ErroreDm::Chiave)
}
fn firma_effimera(k: &Effimera, digest: &[u8; 32], e: &mut dyn Entropia) -> Option<[u8; 64]> {
    let sk = k256::schnorr::SigningKey::from_slice(&k.segreto).ok()?;
    Some(sk.sign_raw(digest, &casuale_32(e)).ok()?.to_bytes())
}

fn digest_di(id: &str) -> Option<[u8; 32]> {
    da_hex32(id)
}

fn adesso() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ─── Costruzione delle buste ─────────────────────────────────────────────────

/// Costruisce le due buste (kind 1059, firmate da chiavi usa e getta, data sfalsata) di un messaggio da
/// `mittente` (ruolo Dm) alla chiave DM `dm_destinatario`.
pub fn incarta(rete: Rete, mittente: &ChiaveRuolo, dm_destinatario: &str, testo: &str, ora: u64, entropia: &mut dyn Entropia) -> Result<Buste, ErroreDm> {
    let (per_destinatario, per_mittente) = costruisci(None, rete, mittente, dm_destinatario, testo, ora, entropia)?;
    Ok(Buste { per_destinatario, per_mittente })
}

/// Una sola costruzione per l'uscita vera e per le buste forgiate delle prove: ciò che le prove scartano è
/// fatto con lo stesso codice che produce quelle buone, con UN difetto in più.
pub(crate) fn costruisci(
    guasto: Option<Guasto>,
    rete: Rete,
    mittente: &ChiaveRuolo,
    dm_destinatario: &str,
    testo: &str,
    ora: u64,
    e: &mut dyn Entropia,
) -> Result<(Evento, Evento), ErroreDm> {
    let dest = da_hex32(dm_destinatario).ok_or(ErroreDm::Chiave)?;
    let mia = mittente.pubblica_hex();
    let mia_bytes = da_hex32(&mia).ok_or(ErroreDm::Chiave)?;
    if testo.is_empty() || testo.len() > 65535 {
        return Err(ErroreDm::Messaggio("il testo deve avere da 1 a 65535 byte"));
    }

    // 1. messaggio (kind 14, NON firmato)
    let mut p_destinatario = dm_destinatario.to_string();
    let mut kind_dichiarato = KIND_MESSAGGIO;
    let mut ora_messaggio = ora;
    if guasto == Some(Guasto::DestinatarioSbagliato) {
        p_destinatario = effimera(e)?.pubblica.clone();
    }
    if guasto == Some(Guasto::KindDelMessaggioSbagliato) {
        kind_dichiarato = 1;
    }
    if guasto == Some(Guasto::OraNelFuturo) {
        ora_messaggio = ora + 631_152_000;
    }
    let tag_p = vec![vec!["p".to_string(), p_destinatario]];
    let mut id_msg = id_evento(&mia, ora_messaggio, KIND_MESSAGGIO, &tag_p, testo);
    if guasto == Some(Guasto::IdDelMessaggioSbagliato) {
        id_msg = cambia_un_carattere(&id_msg);
    }
    let mut messaggio = json!({ "id": id_msg, "pubkey": mia, "created_at": ora_messaggio, "kind": kind_dichiarato, "tags": tag_p, "content": testo });
    if guasto == Some(Guasto::MessaggioFirmato) {
        let sig = mittente.firma_schnorr(&digest_di(&id_msg).ok_or(ErroreDm::Chiave)?).ok_or(ErroreDm::Chiave)?;
        messaggio["sig"] = json!(hex(&sig));
    }
    let messaggio_json = serde_json::to_string(&messaggio).map_err(|_| ErroreDm::Messaggio("serializzazione"))?;

    // 2. per ciascun destinatario: sigillo (kind 13) → busta (kind 1059)
    let mut buste = Vec::with_capacity(2);
    for a_chi in [(dm_destinatario.to_string(), dest), (mia.clone(), mia_bytes)] {
        let (a_chi_hex, a_chi_bytes) = a_chi;
        // sigillo: cifrato dalla chiave DM del mittente per chi lo riceve, firmato dalla stessa chiave
        // (nelle prove, `SigilloDiUnAltro`: sigillo di un'altra chiave, ma cifrato bene, così arriva fino al controllo d'autore)
        let altro = if guasto == Some(Guasto::SigilloDiUnAltro) { Some(effimera(e)?) } else { None };
        let conv = match &altro {
            Some(k) => nip44::chiave_conversazione(&k.segreto, &a_chi_bytes)?,
            None => {
                let mut x = mittente.ecdh_dm(&a_chi_bytes).ok_or(ErroreDm::Chiave)?;
                let c = nip44::chiave_da_condiviso(&x);
                x.zeroize();
                c
            }
        };
        let contenuto_sigillo = nip44::cifra_con_nonce(&conv, &casuale_32(e), &messaggio_json)?;
        let ora_sigillo = if guasto == Some(Guasto::SigilloNelFuturo) { ora + 631_152_000 } else { ora.saturating_sub(sfalsamento(e)) };
        let kind_sigillo = if guasto == Some(Guasto::KindDelSigilloSbagliato) { 14 } else { KIND_SIGILLO };
        let autore_sigillo = altro.as_ref().map(|k| k.pubblica.clone()).unwrap_or_else(|| mia.clone());
        let mut id_sigillo = id_evento(&autore_sigillo, ora_sigillo, KIND_SIGILLO, &[], &contenuto_sigillo);
        if guasto == Some(Guasto::IdDelSigilloSbagliato) {
            id_sigillo = cambia_un_carattere(&id_sigillo);
        }
        let dg = digest_di(&id_sigillo).ok_or(ErroreDm::Chiave)?;
        let mut sig = match &altro {
            Some(k) => firma_effimera(k, &dg, e),
            None => mittente.firma_schnorr(&dg),
        }
        .ok_or(ErroreDm::Chiave)?;
        if guasto == Some(Guasto::FirmaDelSigilloFalsa) {
            sig[0] ^= 1;
        }
        let sigillo = json!({ "id": id_sigillo, "pubkey": autore_sigillo, "created_at": ora_sigillo, "kind": kind_sigillo, "tags": [], "content": contenuto_sigillo, "sig": hex(&sig) });
        let sigillo_json = serde_json::to_string(&sigillo).map_err(|_| ErroreDm::Messaggio("serializzazione"))?;

        // busta: cifrata da una chiave usa e getta per chi la riceve, firmata dalla stessa chiave
        let eff = effimera(e)?;
        let conv_busta = nip44::chiave_conversazione(&eff.segreto, &a_chi_bytes)?;
        let mut contenuto = nip44::cifra_con_nonce(&conv_busta, &casuale_32(e), &sigillo_json)?;
        if matches!(guasto, Some(Guasto::VersioneSconosciuta) | Some(Guasto::MacAlterato)) {
            contenuto = manomette_payload(&contenuto, guasto);
        }
        let ora_busta = ora.saturating_sub(sfalsamento(e));
        let p_busta = if guasto == Some(Guasto::TagPDellaBustaSbagliato) { effimera(e)?.pubblica.clone() } else { a_chi_hex };
        let tags = vec![vec!["p".to_string(), p_busta]];
        let busta = firma_evento(KIND_BUSTA, &contenuto, &tags, ora_busta, rete, &eff.pubblica, |d| firma_effimera(&eff, d, e))
            .map_err(|r| ErroreDm::Protocollo(r.to_string()))?;
        buste.push(busta);
    }
    let per_mittente = buste.pop().expect("due buste");
    let per_destinatario = buste.pop().expect("due buste");
    Ok((per_destinatario, per_mittente))
}

/// Solo per le prove: lo stesso id con un carattere diverso (resta un id ben formato).
fn cambia_un_carattere(id: &str) -> String {
    let (testa, resto) = id.split_at(1);
    format!("{}{resto}", if testa == "0" { "1" } else { "0" })
}

/// Solo per le prove: cambia la versione o il MAC di un payload NIP-44 già cifrato.
fn manomette_payload(payload: &str, guasto: Option<Guasto>) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let mut d = STANDARD.decode(payload).expect("payload appena prodotto");
    match guasto {
        Some(Guasto::VersioneSconosciuta) => d[0] = 3,
        _ => {
            let n = d.len();
            d[n - 1] ^= 1;
        }
    }
    STANDARD.encode(d)
}

// ─── Apertura ────────────────────────────────────────────────────────────────

/// Apre una busta ricevuta con la propria chiave DM e controlla tutto (I-61). Qualunque difetto → `Err`,
/// mai un panico e mai un messaggio mostrato.
pub fn apri(rete: Rete, mia: &ChiaveRuolo, busta: &Value) -> Result<Aperto, ErroreDm> {
    let no = |m: &'static str| ErroreDm::Scartata(m);
    if mia.ruolo() != Ruolo::Dm {
        return Err(no("si apre solo con una chiave DM"));
    }
    // la busta è un evento come gli altri: forma, rete, id, firma (kind 4/13/14 già rifiutati qui)
    let b = verifica(busta, rete).map_err(|_| no("busta non valida"))?;
    if b.kind != KIND_BUSTA {
        return Err(no("non è una busta 1059"));
    }
    let mia_hex = mia.pubblica_hex();
    let destinatari: Vec<&Vec<String>> = b.tags.iter().filter(|t| t[0] == "p").collect();
    if destinatari.len() != 1 || destinatari[0].get(1) != Some(&mia_hex) {
        return Err(no("busta per un altro destinatario"));
    }
    let eff = da_hex32(&b.pubkey).ok_or(no("chiave della busta"))?;
    let mut x = mia.ecdh_dm(&eff).ok_or(no("chiave della busta non valida"))?;
    let conv = nip44::chiave_da_condiviso(&x);
    x.zeroize();
    let sigillo_json = nip44::decifra(&conv, &b.content).map_err(|_| no("busta non decifrabile"))?;

    // sigillo
    let sigillo: Value = serde_json::from_str(&sigillo_json).map_err(|_| no("sigillo non JSON"))?;
    let (s_id, s_pub, s_sig, s_cont) = (campo(&sigillo, "id")?, campo(&sigillo, "pubkey")?, campo(&sigillo, "sig")?, campo(&sigillo, "content")?);
    let s_ora = numero(&sigillo, "created_at")?;
    if sigillo.get("kind").and_then(Value::as_u64) != Some(KIND_SIGILLO as u64) {
        return Err(no("il sigillo non è kind 13"));
    }
    if sigillo.get("tags").and_then(Value::as_array).map(|t| !t.is_empty()) != Some(false) {
        return Err(no("il sigillo ha tag"));
    }
    if id_evento(s_pub, s_ora, KIND_SIGILLO, &[], s_cont) != s_id {
        return Err(no("id del sigillo"));
    }
    verifica_firma(s_pub, s_id, s_sig).map_err(|_| no("firma del sigillo falsa"))?;
    let limite = adesso() + SFASAMENTO_MASSIMO;
    if s_ora > limite {
        return Err(no("sigillo nel futuro"));
    }

    // messaggio
    let dm_mittente = da_hex32(s_pub).ok_or(no("chiave del sigillo"))?;
    let mut y = mia.ecdh_dm(&dm_mittente).ok_or(no("chiave del mittente non valida"))?;
    let conv2 = nip44::chiave_da_condiviso(&y);
    y.zeroize();
    let messaggio_json = nip44::decifra(&conv2, s_cont).map_err(|_| no("sigillo non decifrabile"))?;
    let m: Value = serde_json::from_str(&messaggio_json).map_err(|_| no("messaggio non JSON"))?;
    if m.get("sig").is_some() {
        return Err(no("il messaggio non deve essere firmato"));
    }
    if m.get("kind").and_then(Value::as_u64) != Some(KIND_MESSAGGIO as u64) {
        return Err(no("il messaggio non è kind 14"));
    }
    let (m_id, m_pub, m_testo) = (campo(&m, "id")?, campo(&m, "pubkey")?, campo(&m, "content")?);
    let m_ora = numero(&m, "created_at")?;
    if m_pub != s_pub {
        return Err(no("il sigillo è di un altro autore"));
    }
    if m_testo.is_empty() {
        return Err(no("messaggio vuoto"));
    }
    let m_tags = tag_di(&m)?;
    if id_evento(m_pub, m_ora, KIND_MESSAGGIO, &m_tags, m_testo) != m_id {
        return Err(no("id del messaggio"));
    }
    if m_ora > limite {
        return Err(no("messaggio nel futuro"));
    }
    // il messaggio è per me, oppure l'ho scritto io (la mia copia)
    let ps: Vec<&Vec<String>> = m_tags.iter().filter(|t| t[0] == "p").collect();
    let per_me = ps.len() == 1 && ps[0].get(1) == Some(&mia_hex);
    if !(per_me || s_pub == mia_hex) {
        return Err(no("messaggio per un altro destinatario"));
    }
    if ps.len() != 1 {
        return Err(no("serve un solo destinatario"));
    }
    Ok(Aperto { dm_mittente: s_pub.to_string(), testo: m_testo.to_string(), created_at: m_ora, sigillo_created_at: s_ora })
}

fn campo<'a>(v: &'a Value, k: &'static str) -> Result<&'a str, ErroreDm> {
    v.get(k).and_then(Value::as_str).ok_or(ErroreDm::Scartata("campo mancante o non testo"))
}
fn numero(v: &Value, k: &'static str) -> Result<u64, ErroreDm> {
    v.get(k).and_then(Value::as_u64).filter(|&n| n <= 9_007_199_254_740_991).ok_or(ErroreDm::Scartata("data non valida"))
}
fn tag_di(v: &Value) -> Result<Vec<Vec<String>>, ErroreDm> {
    let no = || ErroreDm::Scartata("tag non validi");
    v.get("tags")
        .and_then(Value::as_array)
        .ok_or_else(no)?
        .iter()
        .map(|t| {
            let t = t.as_array().filter(|t| !t.is_empty()).ok_or_else(no)?;
            t.iter().map(|x| x.as_str().map(str::to_owned).ok_or_else(no)).collect()
        })
        .collect()
}

// ─── Annuncio della chiave DM (I-60) ─────────────────────────────────────────

fn testo_prova(rete: Rete, identita: &str, dm: &str) -> [u8; 32] {
    Sha256::digest(format!("m4tr1x/dm-annuncio/v1:{}:{identita}:{dm}", rete.nome()).as_bytes()).into()
}

/// Annuncio firmato dall'identità: le chiavi DM attive, ognuna con la prova di possesso firmata dalla chiave
/// DM stessa (I-60), e i relay per i DM.
pub fn annuncio_dm(rete: Rete, identita: &ChiaveRuolo, dm_attive: &[&ChiaveRuolo], relay: &[String], ora: u64) -> Result<Evento, ErroreDm> {
    if identita.ruolo() != Ruolo::Identity {
        return Err(ErroreDm::Chiave);
    }
    if dm_attive.is_empty() || dm_attive.len() > MAX_CHIAVI_DM || dm_attive.iter().any(|k| k.ruolo() != Ruolo::Dm) {
        return Err(ErroreDm::Chiave);
    }
    if relay.len() > MAX_RELAY || relay.iter().any(|r| r.is_empty() || r.len() > MAX_LUNGHEZZA_RELAY || r.chars().any(char::is_control)) {
        return Err(ErroreDm::Messaggio("relay non validi"));
    }
    let id_pub = identita.pubblica_hex();
    let mut tags = Vec::new();
    for k in dm_attive {
        let dm_pub = k.pubblica_hex();
        let prova = k.firma_schnorr(&testo_prova(rete, &id_pub, &dm_pub)).ok_or(ErroreDm::Chiave)?;
        tags.push(vec!["dm".to_string(), dm_pub, hex(&prova)]);
    }
    for r in relay {
        tags.push(vec!["relay".to_string(), r.clone()]);
    }
    firma_evento(KIND_ANNUNCIO_DM, "", &tags, ora, rete, &id_pub, |d| identita.firma_schnorr(d)).map_err(|r| ErroreDm::Protocollo(r.to_string()))
}

/// Le chiavi DM che un annuncio dichiara valide per la sua identità: solo quelle con una prova di possesso
/// giusta PER QUELL'IDENTITÀ (le altre si ignorano).
pub fn dm_dichiarate(rete: Rete, annuncio: &Value) -> Result<(String, Vec<String>), ErroreDm> {
    let ev = verifica(annuncio, rete).map_err(|r| ErroreDm::Protocollo(r.to_string()))?;
    if ev.kind != KIND_ANNUNCIO_DM {
        return Err(ErroreDm::Scartata("non è un annuncio DM"));
    }
    let mut chiavi: Vec<String> = Vec::new();
    for t in ev.tags.iter().filter(|t| t[0] == "dm") {
        let (Some(dm), Some(prova)) = (t.get(1), t.get(2)) else { continue };
        if t.len() != 3 || chiavi.len() >= MAX_CHIAVI_DM || chiavi.contains(dm) {
            continue;
        }
        let Some(dg) = da_hex32(&hex(&testo_prova(rete, &ev.pubkey, dm))) else { continue };
        if verifica_firma(dm, &hex(&dg), prova).is_ok() {
            chiavi.push(dm.clone());
        }
    }
    Ok((ev.pubkey, chiavi))
}

/// L'identità a cui attribuire un messaggio sigillato da `dm_mittente`: solo se l'annuncio PIÙ RECENTE di
/// un'identità elenca quella chiave con la prova di possesso. Altrimenti `None` (I-60). Se due identità la
/// rivendicano, `None`: nessun nome inventato.
pub fn identita_del_mittente(rete: Rete, annunci: &[Value], dm_mittente: &str) -> Option<String> {
    use std::collections::HashMap;
    // per ogni identità, l'annuncio valido più recente (a pari data, l'id maggiore: ordine stabile)
    let mut recenti: HashMap<String, (u64, String, Vec<String>)> = HashMap::new();
    for a in annunci {
        let Ok((identita, chiavi)) = dm_dichiarate(rete, a) else { continue };
        let (Some(ora), Some(id)) = (a.get("created_at").and_then(Value::as_u64), a.get("id").and_then(Value::as_str)) else { continue };
        let nuovo = (ora, id.to_string(), chiavi);
        match recenti.get(&identita) {
            Some(v) if (v.0, &v.1) >= (nuovo.0, &nuovo.1) => {}
            _ => {
                recenti.insert(identita, nuovo);
            }
        }
    }
    let mut candidate = recenti.into_iter().filter(|(_, v)| v.2.iter().any(|k| k == dm_mittente)).map(|(i, _)| i);
    let prima = candidate.next()?;
    if candidate.next().is_some() {
        return None;
    }
    Some(prima)
}
