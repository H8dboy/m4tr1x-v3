//! SOLO MISURE: `misura <immagine.bmp> [confronto.bmp …] [-- passo,croma …]`. Legge un BMP a 24 bit (senza
//! compressione), codifica a celle con alcuni passi di dettaglio, decodifica per strati e stampa byte, PSNR (su RGB
//! e sulla sola luce) e tempi. Ogni `confronto.bmp` (es. lo stesso file passato da un JPEG a una certa qualità) viene
//! confrontato con l'originale allo stesso modo, così si legge "quanti byte per quanta qualità" fianco a fianco.
//! Per il primo passo stampa anche la progressione pezzo per pezzo dello strato 2.

use m4tr1x_celle::{codifica, complessita, decodifica, pezzi_dettaglio, psnr_luce_x100, psnr_x100, raggio_per, ycbcr, Codificata, Immagine, Parametri, Strati};
use std::time::Instant;

fn leggi_bmp(p: &str) -> Immagine {
    let b = std::fs::read(p).expect("lettura");
    assert!(b.len() > 54 && &b[0..2] == b"BM", "non è un BMP");
    let inizio = u32::from_le_bytes([b[10], b[11], b[12], b[13]]) as usize;
    let w = i32::from_le_bytes([b[18], b[19], b[20], b[21]]);
    let h = i32::from_le_bytes([b[22], b[23], b[24], b[25]]);
    let bpp = u16::from_le_bytes([b[28], b[29]]);
    let compressione = u32::from_le_bytes([b[30], b[31], b[32], b[33]]);
    assert!(bpp == 24 && compressione == 0, "serve un BMP a 24 bit senza compressione (bpp {bpp}, compressione {compressione})");
    let (w, dal_basso, h) = (w as usize, h > 0, h.unsigned_abs() as usize);
    let riga = (w * 3).div_ceil(4) * 4;
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        let yy = if dal_basso { h - 1 - y } else { y };
        let r = &b[inizio + yy * riga..inizio + yy * riga + w * 3];
        for px in r.chunks_exact(3) {
            rgb.extend_from_slice(&[px[2], px[1], px[0]]);
        }
    }
    Immagine { larghezza: w as u32, altezza: h as u32, rgb }
}

fn db(p: Option<u32>) -> String {
    p.map(|v| format!("{},{:02}", v / 100, v % 100)).unwrap_or_else(|| "-".into())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (file_e_confronti, passi): (&[String], Vec<(u8, u8, u8)>) = match args.iter().position(|a| a == "--") {
        Some(i) => (&args[..i], args[i + 1..].iter().map(|s| {
            let mut it = s.split(',').map(|v| v.parse::<u8>().expect("passo"));
            let p = it.next().unwrap();
            let c = it.next().unwrap_or(p);
            (p, c, it.next().unwrap_or(3))
        }).collect()),
        None => (&args[..], vec![(0, 0, 3), (24, 12, 3), (16, 8, 3), (12, 6, 3), (8, 4, 3), (6, 3, 3), (4, 2, 3)]),
    };
    let file = file_e_confronti.first().expect("immagine .bmp");
    let img = leggi_bmp(file);
    let luce: Vec<u8> = img.rgb.chunks_exact(3).map(|p| ycbcr(p[0], p[1], p[2]).0).collect();
    let c = complessita(&luce, img.larghezza as usize, img.altezza as usize);
    println!("{}x{} · complessità {c} · raggio automatico {} px · grezzo {} byte", img.larghezza, img.altezza, raggio_per(c), img.rgb.len());
    println!("{:<30} {:>7} {:>7} {:>8} {:>9} {:>8} {:>7} {:>7} {:>7} {:>7} {:>7}", "codifica", "testa", "colore", "pendenza", "dettaglio", "totale", "PSNR c", "c+p", "tutto", "luce", "ms c/d");
    let mut primo: Option<(Codificata, Parametri)> = None;
    for (passo, croma, livelli) in passi {
        let par = Parametri { passo_dettaglio: passo, passo_croma: croma, livelli, ..Parametri::default() };
        let t0 = Instant::now();
        let (cod, g) = codifica(&img, par).unwrap();
        let t_cod = t0.elapsed().as_millis();
        let t1 = Instant::now();
        let d0 = decodifica(&cod, Strati::Colore).unwrap();
        let d1 = decodifica(&cod, Strati::ColoreEPendenza).unwrap();
        let d2 = decodifica(&cod, Strati::Tutti).unwrap();
        let t_dec = t1.elapsed().as_millis() / 3;
        let raff = g.raffinate().iter().filter(|&&r| r).count();
        println!(
            "{:<30} {:>7} {:>7} {:>8} {:>9} {:>8} {:>7} {:>7} {:>7} {:>7} {:>3}/{:<3}",
            format!("celle {passo}/{croma}/{livelli} ({} c., {raff} r.)", g.celle()),
            cod.testa.len(),
            cod.colore.len(),
            cod.pendenza.len(),
            cod.dettaglio.len(),
            cod.byte_totali(),
            db(psnr_x100(&img, &d0)),
            db(psnr_x100(&img, &d1)),
            db(psnr_x100(&img, &d2)),
            db(psnr_luce_x100(&img, &d2)),
            t_cod,
            t_dec
        );
        if primo.is_none() && passo > 0 {
            primo = Some((cod, par));
        }
    }
    for confronto in &file_e_confronti[1..] {
        let altro = leggi_bmp(confronto);
        let nome = std::path::Path::new(confronto).file_name().unwrap().to_string_lossy().to_string();
        let jpg = confronto.replace(".bmp", ".jpg");
        let byte = std::fs::metadata(&jpg).map(|m| m.len()).unwrap_or(0);
        println!("{:<30} {:>7} {:>7} {:>8} {:>9} {:>8} {:>7} {:>7} {:>7} {:>7}", nome, "", "", "", "", byte, "", "", db(psnr_x100(&img, &altro)), db(psnr_luce_x100(&img, &altro)));
    }
    if let Some((cod, par)) = primo {
        println!("progressione dello strato 2 (passo {}/{}): byte cumulativi di tutto → PSNR", par.passo_dettaglio, par.passo_croma);
        let fisso = cod.testa.len() + cod.colore.len() + cod.pendenza.len();
        for (k, fine) in pezzi_dettaglio(&cod.dettaglio).iter().enumerate() {
            let parziale = Codificata { dettaglio: cod.dettaglio[..*fine].to_vec(), ..cod.clone() };
            let d = decodifica(&parziale, Strati::Tutti).unwrap();
            println!("  pezzo {k}: {:>8} byte → {} dB (luce {})", fisso + fine, db(psnr_x100(&img, &d)), db(psnr_luce_x100(&img, &d)));
        }
    }
}
