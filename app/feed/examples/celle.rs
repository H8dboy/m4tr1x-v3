//! SOLO PROVE: una foto a celle (alveare) su un nodo vero via Tor, con l'identità di prova PUBBLICA ("abandon … art").
//! `celle <onion> <dati> <foto.bmp | cartella con frame-NNN.bmp> [onion del nodo dei file]`: codifica la foto (o il
//! reel: i primi 24 fotogrammi, una chiave ogni 12 → 2 pezzi) a celle, carica i pezzi (ognuno con la sua
//! autorizzazione firmata, quelli già sul nodo si saltano), pubblica la scheda a celle in Signal, rilegge la sezione e
//! riscarica i pezzi uno per uno misurando quando arriva il primo (l'anteprima / il primo gruppo) e com'è l'immagine
//! dopo ogni pezzo (PSNR contro l'originale; per il reel i fotogrammi leggibili). Stampa una riga JSON.

use m4tr1x_celle::reel::{codifica_reel, ParametriReel};
use m4tr1x_celle::{bmp, codifica, pacchetto, psnr_x100, Parametri};
use m4tr1x_identity::{deriva, seme_da_parole, Ruolo};
use m4tr1x_protocol::{firma_evento, tag_autorizzazione_file, tag_file_a_celle, Rete, Sezione, KIND_AUTORIZZAZIONE_FILE, KIND_FILE, MIME_CELLE_FOTO, MIME_CELLE_REEL};
use m4tr1x_transport::http::{autorizzazione, sha256_hex};
use m4tr1x_transport::tor_process::sha256_file;
use m4tr1x_transport::{OnionV3, TorProcessConfig, TorProcessTransport, Transport};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let k = deriva(&seme_da_parole(&(["abandon"; 23].join(" ") + " art"), "").unwrap(), Ruolo::Identity, 1).unwrap();
    let mut a = std::env::args().skip(1);
    let onion = OnionV3::parse(&a.next().expect("onion del nodo")).expect("onion v3 valido");
    let dati = std::path::PathBuf::from(a.next().expect("cartella dati"));
    let sorgente = std::path::PathBuf::from(a.next().expect("foto BMP o cartella di fotogrammi"));
    let nodo_file = a.next().map(|s| OnionV3::parse(&s).expect("onion v3 del nodo dei file")).unwrap_or_else(|| onion.clone());

    // foto: un BMP; reel: i primi 24 `frame-NNN.bmp` della cartella
    let fotogrammi: Vec<m4tr1x_celle::Immagine> = if sorgente.is_dir() {
        let mut nomi: Vec<_> = std::fs::read_dir(&sorgente).expect("cartella").filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with("frame-") && n.ends_with(".bmp")).unwrap_or(false)).collect();
        nomi.sort();
        nomi.iter().take(24).map(|p| bmp::leggi(&std::fs::read(p).expect("lettura")).expect("BMP a 24 bit")).collect()
    } else {
        vec![bmp::leggi(&std::fs::read(&sorgente).expect("lettura")).expect("BMP a 24 bit")]
    };
    let reel = fotogrammi.len() > 1;
    let img = &fotogrammi[0];
    let mime = if reel { MIME_CELLE_REEL } else { MIME_CELLE_FOTO };

    let t0 = std::time::Instant::now();
    let pezzi = if reel {
        let r = codifica_reel(&fotogrammi, ParametriReel { chiave_ogni: 12, fps: 12, ..ParametriReel::default() }).expect("codifica del reel");
        pacchetto::pezzi_reel(&r)
    } else {
        let (cod, _) = codifica(img, Parametri::default()).expect("codifica");
        pacchetto::pezzi_foto(&cod)
    };
    let ms_codifica = t0.elapsed().as_millis();
    let intero: Vec<u8> = pezzi.concat();
    let sha = sha256_hex(&intero);
    let hash: Vec<String> = pezzi.iter().map(|p| sha256_hex(p)).collect();
    let misure: Vec<u64> = pezzi.iter().map(|p| p.len() as u64).collect();
    let dimensione = intero.len() as u64;

    let bin = std::path::PathBuf::from(std::env::var("M4TR1X_TOR_BIN").expect("M4TR1X_TOR_BIN"));
    let t = TorProcessTransport::new(TorProcessConfig { tor_sha256: sha256_file(&bin).expect("hash di tor"), librerie: vec![], tor_bin: bin, data_dir: dati, bootstrap_timeout: Duration::from_secs(180) });
    if let Err(e) = async { t.avvia().await?; t.bootstrap().await }.await {
        println!("{{\"errore_tor\":{:?}}}", e.to_string());
        t.shutdown().await;
        return;
    }
    let ora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let pk = k.pubblica_hex();
    let tags = tag_file_a_celle(Sezione::Signal, &sha, mime, dimensione, if reel { "Prova reel a celle v3" } else { "Prova foto a celle v3" }, &hash, &misure).unwrap();
    let scheda = firma_evento(KIND_FILE, "Prova dell'alveare dal PC via Tor", &tags, ora, Rete::Prova, &pk, |d| k.firma_schnorr(d)).unwrap();
    let firmate = AtomicU32::new(0);
    let autorizza = |sha_pezzo: &str| -> Result<String, String> {
        firmate.fetch_add(1, Ordering::SeqCst);
        let ora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let aut = firma_evento(KIND_AUTORIZZAZIONE_FILE, "Carico un file", &tag_autorizzazione_file(sha_pezzo, ora, ora + 600).unwrap(), ora, Rete::Prova, &pk, |d| k.firma_schnorr(d)).map_err(|e| e.to_string())?;
        Ok(autorizzazione(&serde_json::to_string(&aut).unwrap()))
    };
    let fette: Vec<&[u8]> = pezzi.iter().map(|p| p.as_slice()).collect();
    let t1 = std::time::Instant::now();
    let caricato = match m4tr1x_feed::carica_pezzi(&t, &nodo_file, &fette, autorizza, |f| eprintln!("{f:?}")).await {
        Ok(()) => "accettato".to_string(),
        Err(m4tr1x_feed::ErroreCarico::Rifiutato { stato, motivo }) => format!("rifiutato {stato}: {motivo}"),
        Err(m4tr1x_feed::ErroreCarico::Autorizzazione(e)) => format!("autorizzazione: {e}"),
        Err(m4tr1x_feed::ErroreCarico::NonRaggiungibile(e)) => format!("non raggiungibile: {}", e.ultimo),
    };
    let secondi_carico = t1.elapsed().as_secs();
    let pubblicata = m4tr1x_feed::pubblica(&t, &onion, &scheda, m4tr1x_feed::BUDGET, |_| {}).await.is_ok();
    let g = m4tr1x_feed::leggi_media(&t, &onion, Rete::Prova, Sezione::Signal, m4tr1x_feed::BUDGET, |_| {}).await;
    let in_sezione = g.as_ref().ok().is_some_and(|g| g.media.iter().any(|m| m.id == scheda.id && m.file.a_celle() && m.file.pezzi == hash && m.file.misure == misure));

    // riscarico pezzo per pezzo: quando arriva il primo e com'è l'immagine dopo ogni pezzo
    let t2 = std::time::Instant::now();
    let fonti = if nodo_file.host() == onion.host() { vec![onion.clone()] } else { vec![nodo_file.clone(), onion.clone()] };
    let attesi: Vec<(String, usize)> = hash.iter().cloned().zip(misure.iter().map(|&m| m as usize)).collect();
    let mut ricevuti: Vec<Vec<u8>> = Vec::new();
    let mut tappe: Vec<String> = Vec::new();
    let avanza = |f: m4tr1x_feed::Fase| eprintln!("{f:?}");
    let esito = m4tr1x_feed::scarica_pezzi(&t, &fonti, &attesi, &avanza, |i, b| {
        ricevuti.push(b.to_vec());
        let (psnr, leggibili) = if reel {
            // ogni pezzo è un gruppo di fotogrammi che si legge da solo: PSNR del primo fotogramma del gruppo
            let (f, _) = pacchetto::apri_reel(b).expect("gruppo del reel");
            let primo = ricevuti[..i].iter().map(|p| pacchetto::apri_reel(p).map(|(f, _)| f.len()).unwrap_or(0)).sum::<usize>();
            (psnr_x100(&fotogrammi[primo], &f[0]).unwrap_or(0), ricevuti.iter().map(|p| pacchetto::apri_reel(p).map(|(f, _)| f.len()).unwrap_or(0)).sum::<usize>())
        } else {
            let viste: Vec<&[u8]> = ricevuti.iter().map(|p| p.as_slice()).collect();
            (pacchetto::apri_foto(&viste).ok().and_then(|d| psnr_x100(img, &d)).unwrap_or(0), 1)
        };
        tappe.push(format!("{{\"pezzo\":{},\"byte\":{},\"secondi\":{},\"psnr_x100\":{psnr},\"fotogrammi_leggibili\":{leggibili}}}", i + 1, b.len(), t2.elapsed().as_secs()));
    })
    .await;
    let identici = ricevuti == pezzi;
    println!(
        "{{\"tipo\":\"{}\",\"misura\":\"{}x{}\",\"fotogrammi\":{},\"pezzi\":{},\"byte\":{dimensione},\"anteprima_byte\":{},\"ms_codifica\":{ms_codifica},\"caricato\":{caricato:?},\"autorizzazioni_firmate\":{},\"secondi_carico\":{secondi_carico},\"scheda_pubblicata\":{pubblicata},\"nella_sezione\":{in_sezione},\"scarico\":{:?},\"pezzi_identici\":{identici},\"tappe\":[{}]}}",
        if reel { "reel" } else { "foto" },
        img.larghezza,
        img.altezza,
        fotogrammi.len(),
        pezzi.len(),
        pezzi[0].len(),
        firmate.load(Ordering::SeqCst),
        esito.map(|_| "ok".to_string()).unwrap_or_else(|e| format!("{e:?}")),
        tappe.join(",")
    );
    t.shutdown().await;
}
