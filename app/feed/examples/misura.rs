//! SOLO PER LE PROVE (tests/integration/feed-via-tor.test.js): legge il feed di un nodo vero via Tor
//! due volte nella stessa sessione e stampa una riga JSON per lettura (secondi, tentativi, post).
//! Lanciato due volte con la stessa cartella dati misura anche "dopo un riavvio dell'app"
//! (punto 46 di docs/RIPRESA.md: tor tiene i descrittori degli onion solo in memoria?).
//!
//! Uso: M4TR1X_TOR_BIN=<tor> cargo run -p m4tr1x-feed --example misura -- <onion> <cartella dati>
//! Qui l'hash di tor si prende dal binario stesso (in CI c'è il tor di Ubuntu): la regola dell'app
//! (hash solo dal lock) la prova I-37 con `config_app`.

use m4tr1x_protocol::Rete;
use m4tr1x_transport::tor_process::sha256_file;
use m4tr1x_transport::{OnionV3, TorProcessConfig, TorProcessTransport, Transport};
use std::time::Duration;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut a = std::env::args().skip(1);
    let onion = OnionV3::parse(&a.next().expect("onion del nodo")).expect("onion v3 valido");
    let dati = std::path::PathBuf::from(a.next().expect("cartella dati"));
    let bin = std::path::PathBuf::from(std::env::var("M4TR1X_TOR_BIN").expect("M4TR1X_TOR_BIN"));
    let t = TorProcessTransport::new(TorProcessConfig {
        tor_sha256: sha256_file(&bin).expect("hash di tor"),
        tor_bin: bin,
        data_dir: dati,
        bootstrap_timeout: Duration::from_secs(180),
    });
    t.avvia().await.expect("avvio di tor");
    let b = t.bootstrap().await.expect("bootstrap").millisecondi;
    println!("{{\"lettura\":\"bootstrap\",\"ms\":{b}}}");
    for lettura in ["primo collegamento", "secondo nella stessa sessione"] {
        // come l'app: budget, poi un minuto di attesa e una ripresa
        let r = m4tr1x_feed::leggi_con_ripresa(&t, &onion, Rete::Prova, m4tr1x_feed::BUDGET, |_| {}).await;
        match r {
            Ok(f) => {
                let ids: Vec<String> = f.post.iter().map(|p| format!("\"{}\"", p.id)).collect();
                println!("{{\"lettura\":\"{lettura}\",\"secondi\":{},\"tentativi\":{},\"scartati\":{},\"ids\":[{}]}}", f.secondi, f.tentativi, f.scartati, ids.join(","));
            }
            Err(e) => println!("{{\"lettura\":\"{lettura}\",\"errore\":{:?},\"secondi\":{},\"tentativi\":{}}}", e.ultimo, e.secondi, e.tentativi),
        }
    }
    t.shutdown().await;
}
