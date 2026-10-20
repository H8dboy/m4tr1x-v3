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

/// nome del profilo di prova: il test controlla che il feed lo mostri sul post
const NOME_PROVA: &str = "Prova M4TR1X";

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut a = std::env::args().skip(1);
    let onion = OnionV3::parse(&a.next().expect("onion del nodo")).expect("onion v3 valido");
    let dati = std::path::PathBuf::from(a.next().expect("cartella dati"));
    let testo = a.next().expect("testo del post");
    let bin = std::path::PathBuf::from(std::env::var("M4TR1X_TOR_BIN").expect("M4TR1X_TOR_BIN"));
    let t = TorProcessTransport::new(TorProcessConfig { tor_sha256: sha256_file(&bin).expect("hash di tor"), tor_bin: bin, data_dir: dati, bootstrap_timeout: Duration::from_secs(180) });
    // bootstrap fermo (es. "fermo al 53%"): rete Tor, la prova lo dichiara fase "tor" (non un panic muto)
    let b = match async { t.avvia().await?; t.bootstrap().await }.await {
        Ok(b) => b.millisecondi,
        Err(e) => {
            println!("{{\"errore_tor\":{:?}}}", e.to_string());
            t.shutdown().await;
            return;
        }
    };
    let _ = b;

    let seme = seme_da_parole(&(["abandon"; 23].join(" ") + " art"), "").unwrap();
    let k = deriva(&seme, Ruolo::Identity, 1).unwrap();
    let ora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let ev = firma_evento(1, &testo, &[], ora, Rete::Prova, &k.pubblica_hex(), |d| k.firma_schnorr(d)).expect("firma");
    // il profilo (kind 0) prima del post: nel feed riletto il post deve avere il nome
    let profilo = m4tr1x_protocol::contenuto_profilo(NOME_PROVA, "profilo di prova").expect("profilo");
    let ev_profilo = firma_evento(0, &profilo, &[], ora, Rete::Prova, &k.pubblica_hex(), |d| k.firma_schnorr(d)).expect("firma del profilo");
    drop(k);
    if let Err(e) = m4tr1x_feed::pubblica(&t, &onion, &ev_profilo, m4tr1x_feed::BUDGET, |_| {}).await {
        println!("{{\"id\":\"{}\",\"errore\":{:?}}}", ev.id, format!("profilo: {e:?}"));
        t.shutdown().await;
        return;
    }

    match m4tr1x_feed::pubblica(&t, &onion, &ev, m4tr1x_feed::BUDGET, |_| {}).await {
        Ok(p) => {
            let feed = m4tr1x_feed::leggi_con_ripresa(&t, &onion, Rete::Prova, m4tr1x_feed::BUDGET, |_| {}).await;
            let mio = feed.as_ref().ok().and_then(|f| f.post.iter().find(|x| x.id == ev.id).cloned());
            let nome = mio.as_ref().and_then(|p| p.nome.clone()).unwrap_or_default();
            println!("{{\"id\":\"{}\",\"secondi\":{},\"tentativi\":{},\"riletto\":{},\"nome\":{:?}}}", ev.id, p.secondi, p.tentativi, mio.is_some(), nome);
        }
        Err(e) => println!("{{\"id\":\"{}\",\"errore\":{:?}}}", ev.id, format!("{e:?}")),
    }
    t.shutdown().await;
}
