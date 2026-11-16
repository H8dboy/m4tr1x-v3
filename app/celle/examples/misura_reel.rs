//! SOLO MISURE: `misura_reel <cartella con frame-NNN.bmp> [scrivi=<cartella>] [-- passo,croma,chiave_ogni,cerca …]`.
//! Codifica la sequenza come reel (prima chiave, poi fotogrammi previsti cella per cella), decodifica, stampa byte
//! per fotogramma, PSNR medio e minimo, tempi, quante celle si muovono; con `scrivi=` salva i fotogrammi decodificati
//! in BMP. Se nella cartella c'è `baseline.txt` (MJPEG / MPEG-4 / H.264 dello stesso pezzo) lo stampa sotto.

use m4tr1x_celle::reel::{codifica_reel, decodifica_reel, Fotogramma, ParametriReel};
use m4tr1x_celle::{bmp, psnr_x100, Immagine, Parametri};
use std::time::Instant;

fn db(p: u32) -> String {
    format!("{},{:02}", p / 100, p % 100)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let sep = args.iter().position(|a| a == "--").unwrap_or(args.len());
    let cartella = args.first().expect("cartella dei fotogrammi");
    let scrivi = args[..sep].iter().find_map(|a| a.strip_prefix("scrivi="));
    let insiemi: Vec<(u8, u8, u16, u8, u16, u8, u8)> = if sep < args.len() {
        args[sep + 1..]
            .iter()
            .map(|s| {
                let v: Vec<u32> = s.split(',').map(|x| x.parse().expect("numero")).collect();
                (v[0] as u8, *v.get(1).unwrap_or(&(v[0] / 2)) as u8, *v.get(2).unwrap_or(&60) as u16, *v.get(3).unwrap_or(&16) as u8, *v.get(4).unwrap_or(&0) as u16, *v.get(5).unwrap_or(&3) as u8, *v.get(6).unwrap_or(&1) as u8)
            })
            .collect()
    } else {
        vec![(12, 6, 60, 16, 0, 3, 1), (12, 6, 60, 16, 0, 3, 0), (16, 8, 60, 16, 0, 3, 1), (16, 8, 60, 16, 0, 3, 0)]
    };

    let mut nomi: Vec<_> = std::fs::read_dir(cartella).expect("cartella").filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with("frame-") && n.ends_with(".bmp")).unwrap_or(false)).collect();
    nomi.sort();
    let fotogrammi: Vec<Immagine> = nomi.iter().map(|p| bmp::leggi(&std::fs::read(p).expect("lettura")).expect("bmp")).collect();
    let n = fotogrammi.len();
    assert!(n > 0, "nessun frame-NNN.bmp in {cartella}");
    let (w, h) = (fotogrammi[0].larghezza, fotogrammi[0].altezza);
    println!("{n} fotogrammi {w}x{h} · grezzo {} byte/fotogramma", w * h * 3);
    println!("{:<24} {:>6} {:>9} {:>8} {:>8} {:>8} {:>8} {:>8} {:>6} {:>6} {:>7} {:>7} {:>6} {:>9}", "passo,croma,chiave,cerca,r,b,m", "chiavi", "totale", "B/fotog.", "B chiave", "B prev.", "B bidir", "vett.", "celle", "div.B", "PSNR m", "min", "mosse", "ms c/d");
    for (i, &(passo, croma, chiave_ogni, cerca, raggio, bidir, mobili)) in insiemi.iter().enumerate() {
        let p = ParametriReel { foto: Parametri { passo_dettaglio: passo, passo_croma: croma, raggio, ..Parametri::default() }, chiave_ogni, cerca, soglia_chiave: 24, bidirezionali: bidir, celle_mobili: mobili != 0 };
        let t0 = Instant::now();
        let r = codifica_reel(&fotogrammi, p).unwrap();
        let tc = t0.elapsed().as_millis();
        let t1 = Instant::now();
        let d = decodifica_reel(&r).unwrap();
        let td = t1.elapsed().as_millis();
        let chiavi = r.fotogrammi.iter().filter(|f| matches!(f, Fotogramma::Chiave(_))).count();
        let b_chiavi: usize = r.fotogrammi.iter().filter(|f| matches!(f, Fotogramma::Chiave(_))).map(|f| f.byte_totali()).sum();
        let b_prev: usize = r.fotogrammi.iter().filter(|f| matches!(f, Fotogramma::Previsto(_))).map(|f| f.byte_totali()).sum();
        let b_vett: usize = r.fotogrammi.iter().filter_map(|f| if let Fotogramma::Previsto(p) = f { Some(p.vettori.len()) } else { None }).sum();
        let np = n - chiavi;
        let psnr: Vec<u32> = fotogrammi.iter().zip(&d).map(|(a, b)| psnr_x100(a, b).unwrap()).collect();
        let bidir_n = r.fotogrammi.iter().filter(|f| matches!(f, Fotogramma::Previsto(p) if p.bidirezionale())).count();
        let b_bidir: usize = r.fotogrammi.iter().filter_map(|f| if let Fotogramma::Previsto(p) = f { if p.bidirezionale() { Some(p.byte_totali()) } else { None } } else { None }).sum();
        let media = psnr.iter().map(|&v| v as u64).sum::<u64>() / n as u64;
        let minimo = *psnr.iter().min().unwrap();
        // celle che si muovono: media sui previsti della quota di vettori diversi da zero
        let mut mosse = 0usize;
        let mut celle = 0usize;
        let b_div: usize = r.fotogrammi.iter().filter_map(|f| if let Fotogramma::Previsto(p) = f { Some(p.divisioni.len()) } else { None }).sum();
        for k in 0..n {
            if matches!(r.fotogrammi[k], Fotogramma::Previsto(_)) {
                let v = r.vettori(k).unwrap();
                mosse += v.iter().filter(|&&s| s != (0, 0)).count();
                celle += v.len();
            }
        }
        let totale = r.in_byte().len();
        println!(
            "{:<24} {:>6} {:>9} {:>8} {:>8} {:>8} {:>8} {:>8} {:>6} {:>6} {:>7} {:>7} {:>5}% {:>4}/{:<4}",
            format!("{passo},{croma},{chiave_ogni},{cerca},{raggio},{bidir},{mobili}"),
            chiavi,
            totale,
            totale / n,
            if chiavi > 0 { b_chiavi / chiavi } else { 0 },
            if np > bidir_n { (b_prev - b_bidir) / (np - bidir_n) } else { 0 },
            if bidir_n > 0 { b_bidir / bidir_n } else { 0 },
            if np > 0 { b_vett / np } else { 0 },
            if np > 0 { celle / np } else { 0 },
            if np > 0 { b_div / np } else { 0 },
            db(media as u32),
            db(minimo),
            if celle > 0 { mosse * 100 / celle } else { 0 },
            tc,
            td
        );
        if i == 0 {
            // andamento fotogramma per fotogramma: byte e PSNR dei primi, di uno a metà e dell'ultimo
            let mut righe = Vec::new();
            for k in (0..n).filter(|&k| k < 6 || k == n / 2 || k == n - 1) {
                righe.push(format!("{k}: {} B {} dB", r.fotogrammi[k].byte_totali(), db(psnr[k])));
            }
            println!("    fotogrammi → {}", righe.join(" · "));
            if let Some(dir) = scrivi {
                std::fs::create_dir_all(dir).expect("cartella di uscita");
                for (k, img) in d.iter().enumerate() {
                    std::fs::write(format!("{dir}/dec-{k:03}.bmp"), bmp::scrivi(img)).expect("scrittura");
                }
                println!("    decodificati scritti in {dir}");
            }
        }
    }
    if let Ok(b) = std::fs::read_to_string(format!("{cartella}/baseline.txt")) {
        println!("basi di confronto (stesso pezzo):");
        for r in b.lines() {
            println!("    {r}");
        }
    }
}
