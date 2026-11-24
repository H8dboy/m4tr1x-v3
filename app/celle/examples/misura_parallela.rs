//! SOLO MISURE: `misura_parallela [cartella con frame-NNN.bmp] [chiave_ogni]`.
//! Codifica lo stesso reel su un filo (`codifica_reel_con`, coda in catena prima di ogni chiave) e a tratti su più
//! fili (`codifica_reel_parallela`), stampa tempi, byte e PSNR dei due. Senza cartella usa una scena sintetica
//! (texture che si sposta, con rumore) da 96 fotogrammi 480×360: vale per i tempi, meno per la qualità.

use m4tr1x_celle::reel::{codifica_reel_con, codifica_reel_parallela, decodifica_reel, ParametriReel};
use m4tr1x_celle::{bmp, psnr_x100, Immagine};
use std::time::Instant;

fn sintetica(n: u32, w: u32, h: u32) -> Vec<Immagine> {
    let mut seme = 12345u32;
    let mut rumore = move || {
        seme = seme.wrapping_mul(1103515245).wrapping_add(12345);
        ((seme >> 16) % 9) as i32 - 4
    };
    (0..n)
        .map(|i| {
            let mut rgb = Vec::with_capacity((w * h * 3) as usize);
            let (dx, dy) = (2 * i as i32, (i as i32) / 2);
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let (u, v) = (x + dx, y + dy);
                    let mattoni = if ((u / 24) + (v / 16)) % 2 == 0 { 60 } else { 0 };
                    let onda = ((u * 3 + v * 2) % 128) as i32;
                    let r = (mattoni + onda + rumore()).clamp(0, 255) as u8;
                    let g = ((v * 255 / h as i32) / 2 + mattoni / 2 + rumore()).clamp(0, 255) as u8;
                    let b = (((u ^ v) & 63) * 2 + rumore()).clamp(0, 255) as u8;
                    rgb.extend_from_slice(&[r, g, b]);
                }
            }
            Immagine { larghezza: w, altezza: h, rgb }
        })
        .collect()
}

fn psnr_medio(a: &[Immagine], b: &[Immagine]) -> f64 {
    a.iter().zip(b).map(|(x, y)| psnr_x100(x, y).unwrap_or(0) as f64 / 100.0).sum::<f64>() / a.len() as f64
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let chiave_ogni: u16 = args.iter().find_map(|a| a.parse().ok()).unwrap_or(24);
    let fotogrammi = match args.iter().find(|a| a.parse::<u16>().is_err()) {
        Some(cartella) => {
            let mut nomi: Vec<_> = std::fs::read_dir(cartella).expect("cartella").filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "bmp")).collect();
            nomi.sort();
            nomi.iter().map(|p| bmp::leggi(&std::fs::read(p).expect("lettura")).expect("bmp")).collect()
        }
        None => sintetica(96, 480, 360),
    };
    let fili = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let p = ParametriReel { chiave_ogni, fps: 12, ..ParametriReel::default() };
    println!("{} fotogrammi {}x{}, chiave ogni {chiave_ogni}, {fili} fili disponibili", fotogrammi.len(), fotogrammi[0].larghezza, fotogrammi[0].altezza);
    let t = Instant::now();
    let uno = codifica_reel_con(&fotogrammi, p, |_| {}).unwrap();
    let t_uno = t.elapsed();
    let t = Instant::now();
    let piu = codifica_reel_parallela(&fotogrammi, p, fili, |_| {}).unwrap();
    let t_piu = t.elapsed();
    for (nome, r, tempo) in [("un filo", &uno, t_uno), ("a tratti", &piu, t_piu)] {
        let d = decodifica_reel(r).unwrap();
        println!("{nome:<9} {:>8.2} s  {:>9} byte  PSNR {:.2} dB", tempo.as_secs_f64(), r.byte_totali(), psnr_medio(&fotogrammi, &d));
    }
    println!("tempo ×{:.2}, byte {:+.1} %", t_uno.as_secs_f64() / t_piu.as_secs_f64(), (piu.byte_totali() as f64 / uno.byte_totali() as f64 - 1.0) * 100.0);
}
