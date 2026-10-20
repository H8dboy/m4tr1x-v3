//! SOLO PER LE PROVE (tests/integration/feed-via-tor.test.js): firma un post con l'identità del seme
//! PUBBLICO di prova, lo pubblica via Tor su un nodo vero (come il comando `pubblica` dell'app) e lo
//! rilegge dal feed. Stampa una riga JSON.
//!
//! Uso: M4TR1X_TOR_BIN=<tor> cargo run -p m4tr1x-feed --example pubblica -- <onion> <cartella dati> <testo>

use m4tr1x_identity::{deriva, seme_da_parole, Ruolo};
use m4tr1x_protocol::{firma_evento, Rete};
use m4tr1x_transport::tor_process::sha256_file;
use m4tr1x_transport::{OnionV3, TorProcessConfig, TorProcessTransport, Transport};
use std::time::Duration;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut a = std::env::args().skip(1);
    let onion = OnionV3::parse(&a.next().expect("onion del nodo")).expect("onion v3 valido");
    let dati = std::path::PathBuf::from(a.next().expect("cartella dati"));
    let testo = a.next().expect("testo del post");
    let bin = std::path::PathBuf::from(std::env::var("M4TR1X_TOR_BIN").expect("M4TR1X_TOR_BIN"));
    let t = TorProcessTransport::new(TorProcessConfig { tor_sha256: sha256_file(&bin).expect("hash di tor"), tor_bin: bin, data_dir: dati, bootstrap_timeout: Duration::from_secs(180) });
    t.avvia().await.expect("avvio di tor");
    t.bootstrap().await.expect("bootstrap");

    let seme = seme_da_parole(&(["abandon"; 23].join(" ") + " art"), "").unwrap();
    let k = deriva(&seme, Ruolo::Identity, 1).unwrap();
    let ora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let ev = firma_evento(1, &testo, &[], ora, Rete::Prova, &k.pubblica_hex(), |d| k.firma_schnorr(d)).expect("firma");
    drop(k);

    match m4tr1x_feed::pubblica(&t, &onion, &ev, m4tr1x_feed::BUDGET, |_| {}).await {
        Ok(p) => {
            let riletto = m4tr1x_feed::leggi_con_ripresa(&t, &onion, Rete::Prova, m4tr1x_feed::BUDGET, |_| {}).await.map(|f| f.post.iter().any(|x| x.id == ev.id));
            println!("{{\"id\":\"{}\",\"secondi\":{},\"tentativi\":{},\"riletto\":{}}}", ev.id, p.secondi, p.tentativi, matches!(riletto, Ok(true)));
        }
        Err(e) => println!("{{\"id\":\"{}\",\"errore\":{:?}}}", ev.id, format!("{e:?}")),
    }
    t.shutdown().await;
}
