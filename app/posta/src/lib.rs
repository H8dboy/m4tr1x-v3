//! Posta: i messaggi privati DENTRO l'app (F5 nell'app). Unisce tre pezzi che restano separati:
//! le buste (`m4tr1x-dm`: NIP-17/59/44, nessuna rete), il portachiavi (`m4tr1x-identity`) e il nodo
//! (`m4tr1x-feed` sopra `m4tr1x-transport`: solo onion, I-36).
//!
//! Regole:
//! - la chiave DM si legge dal portachiavi SOLO qui, per il tempo di aprire o incartare, e si cancella
//!   all'uscita (ZeroizeOnDrop); non attraversa mai un'attesa di rete e non arriva mai al guscio (I-31);
//! - l'identità firma una cosa sola, l'annuncio della chiave DM, passando da `firma_come_identita`;
//! - ciò che il nodo manda NON è creduto: ogni busta passa da `dm::apri` (I-61), ogni annuncio dalla prova
//!   di possesso (I-60); un mittente senza annuncio valido resta "sconosciuto", mai con un nome;
//! - dal nodo esce solo il kind 1059 (buste) e il 10477 (annuncio): mai il testo, mai chi scrive a chi
//!   oltre al destinatario della busta (I-19, I-20).

use m4tr1x_dm as dm;
use m4tr1x_feed::{ErrorePubblica, Fase, NonRaggiungibile};
use m4tr1x_identity::creazione::{identita_salvata, IdentitaPubblica};
use m4tr1x_identity::portachiavi::{carica, firma_come_identita, Portachiavi};
use m4tr1x_identity::Ruolo;
use m4tr1x_protocol::{Evento, Rete};
use m4tr1x_transport::{OnionV3, Transport};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

/// Quante buste e quanti annunci si chiedono al nodo in una volta (il client del relay ne accetta 500).
pub const MASSIMO_BUSTE: u64 = 500;
pub const MASSIMO_ANNUNCI: u64 = 500;
/// Un messaggio scritto dall'app: come un post.
pub const MASSIMO_TESTO: usize = 2000;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ErrorePosta {
    #[error("crea prima la tua identità")]
    SenzaIdentita,
    #[error("portachiavi: {0}")]
    Portachiavi(String),
    #[error("destinatario non valido: serve la sua chiave pubblica (64 caratteri esadecimali)")]
    Destinatario,
    #[error("questa persona non ha ancora attivato i messaggi privati")]
    SenzaAnnuncio,
    #[error("{0}")]
    Testo(&'static str),
    #[error("messaggio non costruito: {0}")]
    Dm(#[from] dm::ErroreDm),
    #[error("il nodo ha rifiutato: {0}")]
    Rifiutato(String),
    #[error("il nodo non risponde ancora ({})", .0.ultimo)]
    NonRaggiungibile(NonRaggiungibile),
}

impl From<NonRaggiungibile> for ErrorePosta {
    fn from(n: NonRaggiungibile) -> Self {
        Self::NonRaggiungibile(n)
    }
}

impl From<ErrorePubblica> for ErrorePosta {
    fn from(e: ErrorePubblica) -> Self {
        match e {
            ErrorePubblica::Rifiutato(m) => Self::Rifiutato(m),
            ErrorePubblica::NonRaggiungibile(n) => Self::NonRaggiungibile(n),
        }
    }
}

/// Un messaggio aperto e controllato.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Messaggio {
    /// l'ho scritto io (è la mia copia)
    pub mio: bool,
    pub testo: String,
    pub creato: u64,
}

/// I messaggi scambiati con una chiave DM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversazione {
    /// Identità dell'altro, SOLO se il suo annuncio più recente rivendica la chiave DM con la prova di
    /// possesso (I-60). `None` = mittente sconosciuto: non gli si può rispondere e non ha un nome.
    pub identita: Option<String>,
    /// Chiave DM dell'altro (pubblica): distingue due sconosciuti tra loro.
    pub dm: String,
    /// Nome dal profilo verificato dell'identità, se c'è.
    pub nome: Option<String>,
    /// dal più vecchio al più recente
    pub messaggi: Vec<Messaggio>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Posta {
    /// dalla conversazione con il messaggio più recente
    pub conversazioni: Vec<Conversazione>,
    /// buste arrivate ma non apribili o non valide (I-61): contate, mai mostrate
    pub scartate: usize,
}

fn chiave_valida(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn adesso() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn io(p: &dyn Portachiavi) -> Result<IdentitaPubblica, ErrorePosta> {
    identita_salvata(p).map_err(|e| ErrorePosta::Portachiavi(e.to_string()))?.ok_or(ErrorePosta::SenzaIdentita)
}

/// La chiave DM corrente dichiarata da `identita`: dal suo annuncio VALIDO più recente (a pari data, id
/// maggiore), ignorando gli annunci di altri autori che un nodo malevolo potrebbe mandare al loro posto.
pub fn chiave_dm_di(rete: Rete, annunci: &[Value], identita: &str) -> Option<String> {
    annunci
        .iter()
        .filter_map(|a| {
            let (autore, chiavi) = dm::dm_dichiarate(rete, a).ok()?;
            let ora = a.get("created_at").and_then(Value::as_u64)?;
            let id = a.get("id").and_then(Value::as_str)?.to_string();
            (autore == identita).then_some((ora, id, chiavi))
        })
        .max_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)))
        .and_then(|(_, _, chiavi)| chiavi.into_iter().next())
}

/// Il mio annuncio (firmato dall'identità, con la prova di possesso della chiave DM), pronto da pubblicare.
/// L'identità firma passando da `firma_come_identita`; la chiave DM vive solo dentro questa funzione.
pub fn mio_annuncio(rete: Rete, p: &dyn Portachiavi, ora: u64) -> Result<Evento, ErrorePosta> {
    let chi = io(p)?;
    let mia = carica(p, Ruolo::Dm, 1).map_err(|e| ErrorePosta::Portachiavi(e.to_string()))?.ok_or(ErrorePosta::SenzaIdentita)?;
    Ok(dm::annuncio_dm_con(rete, &chi.identity, |d| firma_come_identita(p, d).ok(), &[&mia], &[], ora)?)
}

/// Apre le buste con la mia chiave DM e le raggruppa per interlocutore. Nessuna rete qui.
/// Le buste non apribili o non valide si contano e basta.
pub fn apri_tutte(rete: Rete, p: &dyn Portachiavi, buste: &[Value], annunci: &[Value]) -> Result<Posta, ErrorePosta> {
    let chi = io(p)?;
    let mut scartate = 0;
    let mut per_chiave: BTreeMap<String, Vec<Messaggio>> = BTreeMap::new();
    {
        let mia = carica(p, Ruolo::Dm, 1).map_err(|e| ErrorePosta::Portachiavi(e.to_string()))?.ok_or(ErrorePosta::SenzaIdentita)?;
        for b in buste.iter().take(MASSIMO_BUSTE as usize) {
            let Ok(a) = dm::apri(rete, &mia, b) else {
                scartate += 1;
                continue;
            };
            let mio = a.dm_mittente == chi.dm;
            let altro = if mio { a.dm_destinatario } else { a.dm_mittente };
            let m = Messaggio { mio, testo: a.testo, creato: a.created_at };
            let lista = per_chiave.entry(altro).or_default();
            // la stessa busta mandata due volte dal nodo, o un messaggio a me stesso (due copie): una volta sola
            if !lista.contains(&m) {
                lista.push(m);
            }
        }
        // `mia` esce di scena qui: segreto cancellato
    }
    let mut conversazioni: Vec<Conversazione> = per_chiave
        .into_iter()
        .map(|(chiave, mut messaggi)| {
            messaggi.sort_by(|a, b| a.creato.cmp(&b.creato));
            Conversazione { identita: dm::identita_del_mittente(rete, annunci, &chiave), dm: chiave, nome: None, messaggi }
        })
        .collect();
    conversazioni.sort_by(|a, b| b.messaggi.last().map(|m| m.creato).cmp(&a.messaggi.last().map(|m| m.creato)).then_with(|| a.dm.cmp(&b.dm)));
    Ok(Posta { conversazioni, scartate })
}

/// Pubblica il mio annuncio se il nodo non ne ha uno valido che rivendica la mia chiave DM attuale:
/// senza, chi riceve un mio messaggio non può sapere che è mio.
async fn assicura_annuncio<T: Transport>(t: &T, nodo: &OnionV3, rete: Rete, p: &dyn Portachiavi, miei: &[Value], budget: Duration, avanza: &impl Fn(Fase)) -> Result<(), ErrorePosta> {
    let chi = io(p)?;
    if chiave_dm_di(rete, miei, &chi.identity).as_deref() == Some(chi.dm.as_str()) {
        return Ok(());
    }
    let annuncio = mio_annuncio(rete, p, adesso())?;
    m4tr1x_feed::pubblica(t, nodo, &annuncio, budget, avanza).await?;
    Ok(())
}

/// Legge la mia posta dal nodo: buste per la mia chiave DM, annunci per attribuirle, profili per i nomi.
pub async fn leggi<T: Transport>(t: &T, nodo: &OnionV3, rete: Rete, p: &dyn Portachiavi, budget: Duration, avanza: impl Fn(Fase)) -> Result<Posta, ErrorePosta> {
    let chi = io(p)?;
    let filtri = [
        json!({ "kinds": [dm::KIND_BUSTA], "#p": [chi.dm], "limit": MASSIMO_BUSTE }),
        json!({ "kinds": [dm::KIND_ANNUNCIO_DM], "limit": MASSIMO_ANNUNCI }),
        json!({ "kinds": [dm::KIND_ANNUNCIO_DM], "authors": [chi.identity], "limit": 10 }),
    ];
    let mut risposte = m4tr1x_feed::richiedi(t, nodo, &filtri, budget, &avanza).await?;
    let miei = risposte.pop().unwrap_or_default();
    let mut annunci = risposte.pop().unwrap_or_default();
    let buste = risposte.pop().unwrap_or_default();
    assicura_annuncio(t, nodo, rete, p, &miei, budget, &avanza).await?;
    annunci.extend(miei);
    let mut posta = apri_tutte(rete, p, &buste, &annunci)?;
    let conosciuti: Vec<&str> = posta.conversazioni.iter().filter_map(|c| c.identita.as_deref()).collect();
    if !conosciuti.is_empty() {
        // i nomi sono un di più: se il nodo non risponde a questa richiesta la posta si mostra lo stesso
        let filtro = [json!({ "kinds": [0], "authors": conosciuti, "limit": MASSIMO_ANNUNCI })];
        if let Ok(mut r) = m4tr1x_feed::richiedi(t, nodo, &filtro, budget, &avanza).await {
            let nomi = m4tr1x_feed::nomi_dai_profili(r.pop().unwrap_or_default(), rete);
            for c in &mut posta.conversazioni {
                c.nome = c.identita.as_ref().and_then(|i| nomi.get(i).cloned());
            }
        }
    }
    Ok(posta)
}

/// Manda `testo` all'identità `destinatario`: cerca il suo annuncio, incarta per la sua chiave DM e
/// pubblica le due buste (la sua e la mia copia). Al nodo arrivano solo buste kind 1059.
pub async fn invia<T: Transport>(t: &T, nodo: &OnionV3, rete: Rete, p: &dyn Portachiavi, destinatario: &str, testo: &str, budget: Duration, avanza: impl Fn(Fase)) -> Result<Messaggio, ErrorePosta> {
    let testo = testo.trim();
    if testo.is_empty() {
        return Err(ErrorePosta::Testo("il messaggio è vuoto"));
    }
    if testo.chars().count() > MASSIMO_TESTO {
        return Err(ErrorePosta::Testo("il messaggio supera i 2000 caratteri"));
    }
    if !chiave_valida(destinatario) {
        return Err(ErrorePosta::Destinatario);
    }
    let chi = io(p)?;
    let filtri = [
        json!({ "kinds": [dm::KIND_ANNUNCIO_DM], "authors": [destinatario], "limit": 10 }),
        json!({ "kinds": [dm::KIND_ANNUNCIO_DM], "authors": [chi.identity], "limit": 10 }),
    ];
    let mut risposte = m4tr1x_feed::richiedi(t, nodo, &filtri, budget, &avanza).await?;
    let miei = risposte.pop().unwrap_or_default();
    let suoi = risposte.pop().unwrap_or_default();
    let dm_destinatario = chiave_dm_di(rete, &suoi, destinatario).ok_or(ErrorePosta::SenzaAnnuncio)?;
    assicura_annuncio(t, nodo, rete, p, &miei, budget, &avanza).await?;
    let ora = adesso();
    let buste = {
        let mia = carica(p, Ruolo::Dm, 1).map_err(|e| ErrorePosta::Portachiavi(e.to_string()))?.ok_or(ErrorePosta::SenzaIdentita)?;
        dm::incarta(rete, &mia, &dm_destinatario, testo, ora, &mut dm::EntropiaSistema)?
        // `mia` esce di scena qui, prima di ogni attesa di rete
    };
    m4tr1x_feed::pubblica(t, nodo, &buste.per_destinatario, budget, &avanza).await?;
    m4tr1x_feed::pubblica(t, nodo, &buste.per_mittente, budget, &avanza).await?;
    Ok(Messaggio { mio: true, testo: testo.to_string(), creato: ora })
}
