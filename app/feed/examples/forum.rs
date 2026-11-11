//! SOLO PROVE: forum su un nodo vero via Tor, con l'identità di prova PUBBLICA ("abandon … art").
//! Apre una discussione, ci risponde, rilegge il forum e la discussione. Stampa una riga JSON.
//! Uso: M4TR1X_TOR_BIN=<tor> cargo run --example forum -p m4tr1x-feed -- <onion> <cartella dati> <titolo>

use m4tr1x_identity::{deriva, seme_da_parole, Ruolo};
use m4tr1x_protocol::{firma_evento, tag_discussione, tag_risposta, Rete};
use m4tr1x_transport::tor_process::sha256_file;
use m4tr1x_transport::{OnionV3, TorProcessConfig, TorProcessTransport, Transport};
use std::time::Duration;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut a = std::env::args().skip(1);
    let onion = OnionV3::parse(&a.next().expect("onion del nodo")).expect("onion v3 valido");
    let dati = std::path::PathBuf::from(a.next().expect("cartella dati"));
    let titolo = a.next().expect("titolo");
    let bin = std::path::PathBuf::from(std::env::var("M4TR1X_TOR_BIN").expect("M4TR1X_TOR_BIN"));
    let t = TorProcessTransport::new(TorProcessConfig { tor_sha256: sha256_file(&bin).expect("hash di tor"), librerie: vec![], tor_bin: bin, data_dir: dati, bootstrap_timeout: Duration::from_secs(180) });
    if let Err(e) = async { t.avvia().await?; t.bootstrap().await }.await {
        println!("{{\"errore_tor\":{:?}}}", e.to_string());
        t.shutdown().await;
        return;
    }
    let k = deriva(&seme_da_parole(&(["abandon"; 23].join(" ") + " art"), "").unwrap(), Ruolo::Identity, 1).unwrap();
    let ora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let d = firma_evento(1, "Discussione di prova aperta dal PC via Tor.", &tag_discussione(&titolo).unwrap(), ora, Rete::Prova, &k.pubblica_hex(), |x| k.firma_schnorr(x)).unwrap();
    let r = firma_evento(1, "Risposta di prova.", &tag_risposta(&d.id).unwrap(), ora + 1, Rete::Prova, &k.pubblica_hex(), |x| k.firma_schnorr(x)).unwrap();
    drop(k);
    for e in [&d, &r] {
        if let Err(e) = m4tr1x_feed::pubblica(&t, &onion, e, m4tr1x_feed::BUDGET, |_| {}).await {
            println!("{{\"errore\":{:?}}}", format!("{e:?}"));
            t.shutdown().await;
            return;
        }
    }
    let f = m4tr1x_feed::leggi_forum(&t, &onion, Rete::Prova, m4tr1x_feed::BUDGET, |_| {}).await;
    let nel_forum = f.as_ref().ok().and_then(|f| f.discussioni.iter().find(|x| x.id == d.id)).map(|x| (x.titolo.clone(), x.risposte));
    let aperta = m4tr1x_feed::leggi_discussione(&t, &onion, Rete::Prova, &d.id, m4tr1x_feed::BUDGET, |_| {}).await;
    let risposte = aperta.ok().flatten().map(|x| x.risposte.len());
    println!("{{\"nel_forum\":{:?},\"risposte_nella_discussione\":{:?}}}", nel_forum, risposte);
    t.shutdown().await;
}
