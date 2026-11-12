//! SOLO PROVE: file su un nodo vero via Tor, con l'identità di prova PUBBLICA ("abandon … art").
//! `chiave`: stampa la chiave pubblica dell'identità di prova (da mettere in `uploaders` del nodo di prova).
//! `prova <onion> <dati> <file>`: pulisce il file, firma l'autorizzazione, lo carica, pubblica la scheda in Signal,
//! rilegge la sezione e riscarica il file controllando che sia identico a quello pulito. Stampa una riga JSON.

use m4tr1x_identity::{deriva, seme_da_parole, Ruolo};
use m4tr1x_protocol::{firma_evento, tag_autorizzazione_file, tag_file, Rete, Sezione, KIND_AUTORIZZAZIONE_FILE, KIND_FILE};
use m4tr1x_transport::http::{autorizzazione, sha256_hex, Caricato};
use m4tr1x_transport::tor_process::sha256_file;
use m4tr1x_transport::{OnionV3, TorProcessConfig, TorProcessTransport, Transport};
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
    let (tipo, pulito) = m4tr1x_media::ripulisci(&originale).expect("pulizia");
    let mime = tipo.mime(false);
    let sha = sha256_hex(&pulito);
    let bin = std::path::PathBuf::from(std::env::var("M4TR1X_TOR_BIN").expect("M4TR1X_TOR_BIN"));
    let t = TorProcessTransport::new(TorProcessConfig { tor_sha256: sha256_file(&bin).expect("hash di tor"), librerie: vec![], tor_bin: bin, data_dir: dati, bootstrap_timeout: Duration::from_secs(180) });
    if let Err(e) = async { t.avvia().await?; t.bootstrap().await }.await {
        println!("{{\"errore_tor\":{:?}}}", e.to_string());
        t.shutdown().await;
        return;
    }
    let ora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let pk = k.pubblica_hex();
    let aut = firma_evento(KIND_AUTORIZZAZIONE_FILE, "Carico un file", &tag_autorizzazione_file(&sha, ora, ora + 600).unwrap(), ora, Rete::Prova, &pk, |d| k.firma_schnorr(d)).unwrap();
    let scheda = firma_evento(KIND_FILE, "Prova di caricamento dal PC via Tor", &tag_file(Sezione::Signal, &sha, mime, pulito.len() as u64, "Prova file v3").unwrap(), ora, Rete::Prova, &pk, |d| k.firma_schnorr(d)).unwrap();
    drop(k);
    let caricato = m4tr1x_feed::carica_file(&t, &onion, &autorizzazione(&serde_json::to_string(&aut).unwrap()), &pulito, |_| {}).await;
    let caricato = match caricato {
        Ok(Caricato::Accettato) => "accettato".to_string(),
        Ok(Caricato::Rifiutato { stato, motivo }) => format!("rifiutato {stato}: {motivo}"),
        Err(e) => format!("non raggiungibile: {}", e.ultimo),
    };
    let pubblicata = m4tr1x_feed::pubblica(&t, &onion, &scheda, m4tr1x_feed::BUDGET, |_| {}).await.is_ok();
    let g = m4tr1x_feed::leggi_media(&t, &onion, Rete::Prova, Sezione::Signal, m4tr1x_feed::BUDGET, |_| {}).await;
    let in_sezione = g.as_ref().ok().is_some_and(|g| g.media.iter().any(|m| m.id == scheda.id));
    let riscaricato = m4tr1x_feed::scarica_file(&t, &onion, &sha, pulito.len(), |_| {}).await;
    let identico = riscaricato.as_ref().ok().is_some_and(|b| *b == pulito);
    println!("{{\"caricato\":{caricato:?},\"scheda_pubblicata\":{pubblicata},\"nella_sezione\":{in_sezione},\"riscaricato_identico\":{identico},\"byte\":[{},{}]}}", originale.len(), pulito.len());
    t.shutdown().await;
}
