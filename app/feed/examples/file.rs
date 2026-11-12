//! SOLO PROVE: file su un nodo vero via Tor, con l'identità di prova PUBBLICA ("abandon … art").
//! `chiave`: stampa la chiave pubblica dell'identità di prova (da mettere in `uploaders` del nodo di prova).
//! `prova <onion> <dati> <file> [onion del nodo dei file]`: pulisce il file, lo carica A PEZZI (ADR-0005: ogni pezzo
//! con la sua autorizzazione firmata, i pezzi già sul nodo si saltano), pubblica la scheda in Signal, rilegge la
//! sezione e riscarica il file pezzo per pezzo controllando che sia identico a quello pulito. Stampa una riga JSON.
//! Lanciato due volte sullo stesso file, la seconda non ricarica nessun pezzo (`autorizzazioni_firmate` = 0).

use m4tr1x_identity::{deriva, seme_da_parole, Ruolo};
use m4tr1x_protocol::{firma_evento, tag_autorizzazione_file, tag_file, Rete, Sezione, KIND_AUTORIZZAZIONE_FILE, KIND_FILE};
use m4tr1x_transport::http::{autorizzazione, sha256_hex};
use m4tr1x_transport::tor_process::sha256_file;
use m4tr1x_transport::{OnionV3, TorProcessConfig, TorProcessTransport, Transport};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let k = deriva(&seme_da_parole(&(["abandon"; 23].join(" ") + " art"), "").unwrap(), Ruolo::Identity, 1).unwrap();
    let mut a = std::env::args().skip(1);
    if a.next().as_deref() == Some("chiave") {
        println!("{}", k.pubblica_hex());
        return;
    }
    let onion = OnionV3::parse(&a.next().expect("onion del nodo")).expect("onion v3 valido");
    let dati = std::path::PathBuf::from(a.next().expect("cartella dati"));
    let originale = std::fs::read(a.next().expect("file")).expect("lettura");
    let nodo_file = a.next().map(|s| OnionV3::parse(&s).expect("onion v3 del nodo dei file")).unwrap_or_else(|| onion.clone());
    let (tipo, pulito) = m4tr1x_media::ripulisci(&originale).expect("pulizia");
    let mime = tipo.mime(false);
    let sha = sha256_hex(&pulito);
    let pezzi = m4tr1x_feed::hash_dei_pezzi(&pulito);
    let bin = std::path::PathBuf::from(std::env::var("M4TR1X_TOR_BIN").expect("M4TR1X_TOR_BIN"));
    let t = TorProcessTransport::new(TorProcessConfig { tor_sha256: sha256_file(&bin).expect("hash di tor"), librerie: vec![], tor_bin: bin, data_dir: dati, bootstrap_timeout: Duration::from_secs(180) });
    if let Err(e) = async { t.avvia().await?; t.bootstrap().await }.await {
        println!("{{\"errore_tor\":{:?}}}", e.to_string());
        t.shutdown().await;
        return;
    }
    let ora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let pk = k.pubblica_hex();
    let scheda = firma_evento(KIND_FILE, "Prova di caricamento a pezzi dal PC via Tor", &tag_file(Sezione::Signal, &sha, mime, pulito.len() as u64, "Prova file a pezzi v3", &pezzi).unwrap(), ora, Rete::Prova, &pk, |d| k.firma_schnorr(d)).unwrap();
    let firmate = AtomicU32::new(0);
    let autorizza = |sha_pezzo: &str| -> Result<String, String> {
        firmate.fetch_add(1, Ordering::SeqCst);
        let ora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let aut = firma_evento(KIND_AUTORIZZAZIONE_FILE, "Carico un file", &tag_autorizzazione_file(sha_pezzo, ora, ora + 600).unwrap(), ora, Rete::Prova, &pk, |d| k.firma_schnorr(d)).map_err(|e| e.to_string())?;
        Ok(autorizzazione(&serde_json::to_string(&aut).unwrap()))
    };
    let t0 = std::time::Instant::now();
    let caricato = match m4tr1x_feed::carica_file_a_pezzi(&t, &nodo_file, &pulito, autorizza, |f| eprintln!("{f:?}")).await {
        Ok(()) => "accettato".to_string(),
        Err(m4tr1x_feed::ErroreCarico::Rifiutato { stato, motivo }) => format!("rifiutato {stato}: {motivo}"),
        Err(m4tr1x_feed::ErroreCarico::Autorizzazione(e)) => format!("autorizzazione: {e}"),
        Err(m4tr1x_feed::ErroreCarico::NonRaggiungibile(e)) => format!("non raggiungibile: {}", e.ultimo),
    };
    let secondi_carico = t0.elapsed().as_secs();
    let pubblicata = m4tr1x_feed::pubblica(&t, &onion, &scheda, m4tr1x_feed::BUDGET, |_| {}).await.is_ok();
    let g = m4tr1x_feed::leggi_media(&t, &onion, Rete::Prova, Sezione::Signal, m4tr1x_feed::BUDGET, |_| {}).await;
    let in_sezione = g.as_ref().ok().is_some_and(|g| g.media.iter().any(|m| m.id == scheda.id && m.file.pezzi == pezzi));
    let t1 = std::time::Instant::now();
    let fonti = if nodo_file.host() == onion.host() { vec![onion.clone()] } else { vec![nodo_file.clone(), onion.clone()] };
    let riscaricato = m4tr1x_feed::scarica_file_a_pezzi(&t, &fonti, &sha, pulito.len() as u64, &pezzi, |f| eprintln!("{f:?}")).await;
    let identico = riscaricato.as_ref().ok().is_some_and(|b| *b == pulito);
    println!(
        "{{\"caricato\":{caricato:?},\"pezzi\":{},\"autorizzazioni_firmate\":{},\"secondi_carico\":{secondi_carico},\"scheda_pubblicata\":{pubblicata},\"nella_sezione\":{in_sezione},\"riscaricato_identico\":{identico},\"secondi_scarico\":{},\"byte\":[{},{}]}}",
        m4tr1x_feed::fette(&pulito).len(),
        firmate.load(Ordering::SeqCst),
        t1.elapsed().as_secs(),
        originale.len(),
        pulito.len()
    );
    t.shutdown().await;
}
