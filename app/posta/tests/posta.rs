//! Posta dell'app con un nodo finto (niente rete, niente Tor) e portachiavi in memoria: due utenti di prova
//! (semi PUBBLICI di prova) si scrivono passando dalle stesse funzioni che usa il guscio.
//! Cosa si prova: al nodo arrivano solo buste 1059 e annunci 10477, mai il testo (I-19); un mittente
//! senza annuncio valido resta sconosciuto (I-60); le buste forgiate si contano e non si mostrano (I-61);
//! un nodo malevolo che risponde con l'annuncio di un altro non fa cifrare per la chiave sbagliata.

use m4tr1x_dm::prova::{incarta_difettosa, Difetto};
use m4tr1x_dm::{annuncio_dm, incarta, Entropia, DURATA_ANNUNCIO, KIND_ANNUNCIO_DM, KIND_BUSTA, RINNOVO_ANNUNCIO};
use m4tr1x_identity::creazione::recupera;
use m4tr1x_identity::portachiavi::Memoria;
use m4tr1x_identity::{deriva, seme_da_parole, ChiaveRuolo, Ruolo};
use m4tr1x_posta::{chiave_dm_di, da_rinnovare, invia, leggi, ErrorePosta, MASSIMO_TESTO};
use m4tr1x_protocol::Rete;
use m4tr1x_transport::prova::{relay_finto_che_ignora_il_filtro, relay_finto_condiviso};
use m4tr1x_transport::{BootstrapInfo, Isolation, OnionV3, Stream, Transport, TransportStatus};
use serde_json::Value;
use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const RETE: Rete = Rete::Prova;
const BUDGET: Duration = Duration::from_secs(60);
const SEGRETO: &str = "ci vediamo alle sette al solito posto";

fn parole(a: &str, ultima: &str) -> String {
    [a; 23].join(" ") + " " + ultima
}
fn parole_anna() -> String {
    parole("abandon", "art")
}
fn parole_bruno() -> String {
    parole("zoo", "vote")
}
/// Vettore BIP-39 pubblico a 24 parole: la terza persona delle prove.
const PAROLE_CARLA: &str = "legal winner thank year wave sausage worth useful legal winner thank year wave sausage worth useful legal winner thank year wave sausage worth title";

struct Utente {
    portachiavi: Memoria,
    identita: String,
    dm: String,
}

fn utente(parole: &str) -> Utente {
    let portachiavi = Memoria::default();
    let id = recupera(parole, &portachiavi).unwrap();
    Utente { portachiavi, identita: id.identity, dm: id.dm }
}

fn chiavi(parole: &str, versione_dm: u32) -> (ChiaveRuolo, ChiaveRuolo) {
    let seme = seme_da_parole(parole, "").unwrap();
    (deriva(&seme, Ruolo::Identity, 1).unwrap(), deriva(&seme, Ruolo::Dm, versione_dm).unwrap())
}

struct Contatore(u64);
impl Entropia for Contatore {
    fn riempi(&mut self, dove: &mut [u8]) {
        for b in dove.iter_mut() {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            *b = (self.0 >> 33) as u8;
        }
    }
}

fn adesso() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}

/// Nodo finto: gli eventi sono condivisi tra le connessioni, come su un nodo vero.
struct Nodo {
    eventi: Arc<Mutex<Vec<Value>>>,
    chiamate: AtomicU32,
    /// nodo malevolo: ignora i filtri e manda ogni evento
    ignora_filtro: bool,
}

impl Nodo {
    fn nuovo() -> Self {
        Self { eventi: Default::default(), chiamate: AtomicU32::new(0), ignora_filtro: false }
    }
    fn malevolo() -> Self {
        Self { ignora_filtro: true, ..Self::nuovo() }
    }
    fn eventi(&self) -> Vec<Value> {
        self.eventi.lock().unwrap().clone()
    }
    fn aggiungi(&self, e: Value) {
        self.eventi.lock().unwrap().push(e)
    }
}

impl Transport for Nodo {
    fn bootstrap(&self) -> impl Future<Output = m4tr1x_transport::Result<BootstrapInfo>> + Send {
        async { Ok(BootstrapInfo { millisecondi: 0, implementazione: "finto" }) }
    }
    fn connect_onion(&self, _h: &OnionV3, _p: u16, _i: Isolation) -> impl Future<Output = m4tr1x_transport::Result<Stream>> + Send {
        self.chiamate.fetch_add(1, Ordering::SeqCst);
        let (eventi, ignora) = (self.eventi.clone(), self.ignora_filtro);
        async move {
            let (client, server) = tokio::io::duplex(1 << 20);
            if ignora {
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
    OnionV3::from_pubkey([7u8; 32])
}

#[tokio::test]
async fn due_utenti_si_scrivono_e_al_nodo_arrivano_solo_buste_e_annunci() {
    let (nodo, anna, bruno) = (Nodo::nuovo(), utente(&parole_anna()), utente(&parole_bruno()));
    // Bruno apre la posta: vuota, e così pubblica il suo annuncio
    let vuota = leggi(&nodo, &onion(), RETE, &bruno.portachiavi, BUDGET, |_| {}).await.unwrap();
    assert!(vuota.conversazioni.is_empty());
    assert_eq!(chiave_dm_di(RETE, &nodo.eventi(), &bruno.identita, adesso()).as_deref(), Some(bruno.dm.as_str()));

    let inviato = invia(&nodo, &onion(), RETE, &anna.portachiavi, &bruno.identita, SEGRETO, BUDGET, |_| {}).await.unwrap();
    assert!(inviato.mio);

    // I-19: sul nodo solo buste e annunci, e il testo non compare in nessun evento
    let sul_nodo = nodo.eventi();
    assert!(sul_nodo.iter().all(|e| [KIND_BUSTA as u64, KIND_ANNUNCIO_DM as u64].contains(&e["kind"].as_u64().unwrap())), "kind inattesi sul nodo");
    assert_eq!(sul_nodo.iter().filter(|e| e["kind"] == KIND_BUSTA as u64).count(), 2, "una busta per il destinatario e una copia per il mittente");
    let tutto = serde_json::to_string(&sul_nodo).unwrap();
    assert!(!tutto.contains(SEGRETO) && !tutto.contains("sette"), "il testo è arrivato in chiaro al nodo");
    // le buste non portano né l'identità né la chiave DM di chi scrive
    for b in sul_nodo.iter().filter(|e| e["kind"] == KIND_BUSTA as u64) {
        let s = b.to_string();
        assert!(!s.contains(&anna.identita), "la busta rivela l'identità del mittente");
        assert_ne!(b["pubkey"], Value::String(anna.dm.clone()), "la busta è firmata dalla chiave DM del mittente");
    }

    // Bruno legge: il messaggio è attribuito ad Anna perché il SUO annuncio rivendica la chiave del sigillo
    let posta = leggi(&nodo, &onion(), RETE, &bruno.portachiavi, BUDGET, |_| {}).await.unwrap();
    // il nodo filtra per destinatario (#p, come quello vero): la copia di Anna, cifrata per lei, a Bruno non arriva
    assert_eq!(posta.scartate, 0);
    assert_eq!(posta.conversazioni.len(), 1);
    let c = &posta.conversazioni[0];
    assert_eq!(c.identita.as_deref(), Some(anna.identita.as_str()));
    assert_eq!((c.messaggi.len(), c.messaggi[0].mio, c.messaggi[0].testo.as_str()), (1, false, SEGRETO));

    // Anna rilegge la sua copia: stessa conversazione, vista dalla sua parte
    let posta = leggi(&nodo, &onion(), RETE, &anna.portachiavi, BUDGET, |_| {}).await.unwrap();
    let c = &posta.conversazioni[0];
    assert_eq!(c.identita.as_deref(), Some(bruno.identita.as_str()));
    assert_eq!((c.messaggi.len(), c.messaggi[0].mio, c.messaggi[0].testo.as_str()), (1, true, SEGRETO));

    // Bruno risponde: la conversazione di Anna ora ha due messaggi, in ordine
    invia(&nodo, &onion(), RETE, &bruno.portachiavi, &anna.identita, "va bene", BUDGET, |_| {}).await.unwrap();
    let posta = leggi(&nodo, &onion(), RETE, &anna.portachiavi, BUDGET, |_| {}).await.unwrap();
    assert_eq!(posta.conversazioni.len(), 1);
    assert_eq!(posta.conversazioni[0].messaggi.iter().map(|m| m.mio).collect::<Vec<_>>(), vec![true, false]);
}

#[tokio::test]
async fn senza_annuncio_del_destinatario_non_parte_niente() {
    let (nodo, anna, bruno) = (Nodo::nuovo(), utente(&parole_anna()), utente(&parole_bruno()));
    let e = invia(&nodo, &onion(), RETE, &anna.portachiavi, &bruno.identita, SEGRETO, BUDGET, |_| {}).await.unwrap_err();
    assert_eq!(e, ErrorePosta::SenzaAnnuncio);
    assert!(nodo.eventi().iter().all(|e| e["kind"] != KIND_BUSTA as u64), "una busta è partita senza sapere per chi cifrare");
}

#[tokio::test]
async fn testo_o_destinatario_non_validi_si_fermano_prima_della_rete() {
    let (nodo, anna, bruno) = (Nodo::nuovo(), utente(&parole_anna()), utente(&parole_bruno()));
    let lungo = "a".repeat(MASSIMO_TESTO + 1);
    for (a, testo) in [(bruno.identita.as_str(), "   "), (bruno.identita.as_str(), lungo.as_str()), ("non-una-chiave", "ciao"), (&bruno.identita.to_uppercase(), "ciao")] {
        assert!(invia(&nodo, &onion(), RETE, &anna.portachiavi, a, testo, BUDGET, |_| {}).await.is_err());
    }
    // senza identità non si legge e non si scrive
    let nessuno = Memoria::default();
    assert_eq!(leggi(&nodo, &onion(), RETE, &nessuno, BUDGET, |_| {}).await.unwrap_err(), ErrorePosta::SenzaIdentita);
    assert_eq!(invia(&nodo, &onion(), RETE, &nessuno, &bruno.identita, "ciao", BUDGET, |_| {}).await.unwrap_err(), ErrorePosta::SenzaIdentita);
    assert_eq!(nodo.chiamate.load(Ordering::SeqCst), 0, "si è aperta una connessione per una richiesta non valida");
}

#[tokio::test]
async fn mittente_senza_annuncio_resta_sconosciuto_e_le_buste_forgiate_non_si_mostrano() {
    let (nodo, bruno) = (Nodo::nuovo(), utente(&parole_bruno()));
    let ora = adesso();
    let mut e = Contatore(1);
    // una chiave DM che nessuna identità ha annunciato (la versione 2 di Anna)
    let (_, ignota) = chiavi(&parole_anna(), 2);
    nodo.aggiungi(serde_json::to_value(incarta(RETE, &ignota, &bruno.dm, "chi sono?", ora, &mut e).unwrap().per_destinatario).unwrap());
    // buste forgiate: sigillo di un altro, firma falsa, messaggio per un altro
    let (_, dm_anna) = chiavi(&parole_anna(), 1);
    for d in [Difetto::SigilloDiUnAltro, Difetto::FirmaDelSigilloFalsa, Difetto::DestinatarioSbagliato, Difetto::MacAlterato] {
        nodo.aggiungi(incarta_difettosa(d, RETE, &dm_anna, &bruno.dm, "FORGIATO", ora, &mut e).unwrap());
    }
    let posta = leggi(&nodo, &onion(), RETE, &bruno.portachiavi, BUDGET, |_| {}).await.unwrap();
    assert_eq!(posta.scartate, 4);
    assert_eq!(posta.conversazioni.len(), 1);
    let c = &posta.conversazioni[0];
    assert_eq!((c.identita.as_ref(), c.nome.as_ref()), (None, None), "un mittente senza annuncio ha ricevuto un'identità o un nome");
    assert_eq!(c.messaggi[0].testo, "chi sono?");
    assert!(!format!("{posta:?}").contains("FORGIATO"));
}

#[tokio::test]
async fn un_annuncio_che_rivendica_la_chiave_di_un_altro_non_ruba_il_nome() {
    let (nodo, anna, bruno) = (Nodo::nuovo(), utente(&parole_anna()), utente(&parole_bruno()));
    leggi(&nodo, &onion(), RETE, &bruno.portachiavi, BUDGET, |_| {}).await.unwrap();
    invia(&nodo, &onion(), RETE, &anna.portachiavi, &bruno.identita, SEGRETO, BUDGET, |_| {}).await.unwrap();
    // Carla annuncia la SUA chiave e ci aggiunge a mano la chiave DM di Anna, senza poterne fare la prova
    let (id_carla, dm_carla) = chiavi(PAROLE_CARLA, 1);
    let mut falso = serde_json::to_value(annuncio_dm(RETE, &id_carla, &[&dm_carla], &[], adesso() + 5).unwrap()).unwrap();
    let prova_di_carla = falso["tags"][0][2].clone();
    falso["tags"].as_array_mut().unwrap().insert(0, serde_json::json!(["dm", anna.dm, prova_di_carla]));
    nodo.aggiungi(falso); // id e firma non tornano più: scartato già dalla verifica dell'evento
    let posta = leggi(&nodo, &onion(), RETE, &bruno.portachiavi, BUDGET, |_| {}).await.unwrap();
    assert_eq!(posta.conversazioni[0].identita.as_deref(), Some(anna.identita.as_str()));
}

#[tokio::test]
async fn un_nodo_che_risponde_con_l_annuncio_di_un_altro_non_fa_cifrare_per_la_chiave_sbagliata() {
    let (nodo, anna, bruno) = (Nodo::malevolo(), utente(&parole_anna()), utente(&parole_bruno()));
    leggi(&nodo, &onion(), RETE, &bruno.portachiavi, BUDGET, |_| {}).await.unwrap();
    // l'annuncio (valido, ma di Carla) più recente di tutti: il nodo lo manda a chi chiede quello di Bruno
    let (id_carla, dm_carla) = chiavi(PAROLE_CARLA, 1);
    nodo.aggiungi(serde_json::to_value(annuncio_dm(RETE, &id_carla, &[&dm_carla], &[], adesso() + 60).unwrap()).unwrap());
    invia(&nodo, &onion(), RETE, &anna.portachiavi, &bruno.identita, SEGRETO, BUDGET, |_| {}).await.unwrap();
    let destinatari: Vec<String> = nodo.eventi().iter().filter(|e| e["kind"] == KIND_BUSTA as u64).map(|e| e["tags"].as_array().unwrap().iter().find(|t| t[0] == "p").unwrap()[1].as_str().unwrap().to_string()).collect();
    assert_eq!(destinatari.len(), 2);
    assert!(destinatari.contains(&bruno.dm) && destinatari.contains(&anna.dm), "buste per {destinatari:?}");
    assert!(!destinatari.contains(&dm_carla.pubblica_hex()), "il messaggio è stato cifrato per la chiave di un altro");
    // e Bruno lo legge davvero
    let posta = leggi(&nodo, &onion(), RETE, &bruno.portachiavi, BUDGET, |_| {}).await.unwrap();
    assert!(posta.conversazioni.iter().any(|c| c.identita.as_deref() == Some(anna.identita.as_str()) && c.messaggi[0].testo == SEGRETO));
}

/// Annunci di Bruno sul nodo (kind 10477, firmati dalla sua identità).
fn annunci_di(nodo: &Nodo, identita: &str) -> Vec<Value> {
    nodo.eventi().into_iter().filter(|e| e["kind"] == KIND_ANNUNCIO_DM as u64 && e["pubkey"] == identita).collect()
}

#[tokio::test]
async fn l_annuncio_in_scadenza_si_rinnova_da_solo_quello_fresco_no() {
    let (nodo, bruno) = (Nodo::nuovo(), utente(&parole_bruno()));
    let (id_b, dm_b) = chiavi(&parole_bruno(), 1);
    let ora = adesso();
    // annuncio fresco (di ieri): leggendo la posta non se ne pubblica un altro
    nodo.aggiungi(serde_json::to_value(annuncio_dm(RETE, &id_b, &[&dm_b], &[], ora - 24 * 3600).unwrap()).unwrap());
    assert_eq!(da_rinnovare(RETE, &nodo.eventi(), &bruno.identita, &bruno.dm, ora), None);
    leggi(&nodo, &onion(), RETE, &bruno.portachiavi, BUDGET, |_| {}).await.unwrap();
    assert_eq!(annunci_di(&nodo, &bruno.identita).len(), 1, "ripubblicato un annuncio ancora fresco");
    // annuncio che scade tra meno di RINNOVO_ANNUNCIO: si rinnova
    let nodo = Nodo::nuovo();
    let vecchio = ora - (DURATA_ANNUNCIO - RINNOVO_ANNUNCIO) - 60;
    nodo.aggiungi(serde_json::to_value(annuncio_dm(RETE, &id_b, &[&dm_b], &[], vecchio).unwrap()).unwrap());
    assert_eq!(da_rinnovare(RETE, &nodo.eventi(), &bruno.identita, &bruno.dm, ora), Some("annuncio in scadenza"));
    leggi(&nodo, &onion(), RETE, &bruno.portachiavi, BUDGET, |_| {}).await.unwrap();
    let annunci = annunci_di(&nodo, &bruno.identita);
    assert_eq!(annunci.len(), 2, "annuncio in scadenza non rinnovato");
    assert_eq!(da_rinnovare(RETE, &annunci, &bruno.identita, &bruno.dm, adesso()), None, "il nuovo vale 30 giorni");
}

#[tokio::test]
async fn un_annuncio_senza_scadenza_di_prima_si_rifa_e_non_vale_per_chi_scrive() {
    // gli annunci pubblicati prima del 24/11/2026 non hanno `expiration`: non valgono più
    let (nodo, anna, bruno) = (Nodo::nuovo(), utente(&parole_anna()), utente(&parole_bruno()));
    let (id_b, dm_b) = chiavi(&parole_bruno(), 1);
    let con = serde_json::to_value(annuncio_dm(RETE, &id_b, &[&dm_b], &[], adesso() - 3600).unwrap()).unwrap();
    let tags: Vec<Vec<String>> = con["tags"].as_array().unwrap().iter().filter(|t| t[0] != "net" && t[0] != "expiration").map(|t| t.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect()).collect();
    let senza = m4tr1x_protocol::firma_evento(KIND_ANNUNCIO_DM, "", &tags, adesso() - 3600, RETE, &id_b.pubblica_hex(), |d| id_b.firma_schnorr(d)).unwrap();
    nodo.aggiungi(serde_json::to_value(&senza).unwrap());
    let e = invia(&nodo, &onion(), RETE, &anna.portachiavi, &bruno.identita, SEGRETO, BUDGET, |_| {}).await.unwrap_err();
    assert_eq!(e, ErrorePosta::SenzaAnnuncio, "si è cifrato per un annuncio senza scadenza");
    assert_eq!(da_rinnovare(RETE, &nodo.eventi(), &bruno.identita, &bruno.dm, adesso()), Some("nessun annuncio valido"));
    // Bruno apre la posta: l'app pubblica l'annuncio nuovo, e ora Anna può scrivergli
    leggi(&nodo, &onion(), RETE, &bruno.portachiavi, BUDGET, |_| {}).await.unwrap();
    invia(&nodo, &onion(), RETE, &anna.portachiavi, &bruno.identita, SEGRETO, BUDGET, |_| {}).await.unwrap();
}

#[tokio::test]
async fn a_chi_ha_l_annuncio_scaduto_non_si_scrive() {
    let (nodo, anna, bruno) = (Nodo::nuovo(), utente(&parole_anna()), utente(&parole_bruno()));
    let (id_b, dm_b) = chiavi(&parole_bruno(), 1);
    nodo.aggiungi(serde_json::to_value(annuncio_dm(RETE, &id_b, &[&dm_b], &[], adesso() - DURATA_ANNUNCIO - 60).unwrap()).unwrap());
    let e = invia(&nodo, &onion(), RETE, &anna.portachiavi, &bruno.identita, SEGRETO, BUDGET, |_| {}).await.unwrap_err();
    assert_eq!(e, ErrorePosta::SenzaAnnuncio);
    assert!(nodo.eventi().iter().all(|e| e["kind"] != KIND_BUSTA as u64));
}
