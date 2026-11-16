//! SOLO MISURE: errore medio per canale su un gradiente sintetico, per capire dove si perde qualità.
use m4tr1x_celle::{codifica, decodifica, psnr_x100, ycbcr, Immagine, Parametri, Strati};

fn main() {
    let (w, h) = (160u32, 120u32);
    let mut rgb = Vec::new();
    for y in 0..h {
        for x in 0..w {
            rgb.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, 128]);
        }
    }
    let img = Immagine { larghezza: w, altezza: h, rgb };
    let (c, g) = codifica(&img, Parametri::default()).unwrap();
    println!("celle {} raffinate {} raggio {}", g.celle(), g.raffinate().iter().filter(|&&r| r).count(), g.raggio);
    for (nome, s) in [("colore", Strati::Colore), ("colore+pendenza", Strati::ColoreEPendenza), ("tutti", Strati::Tutti)] {
        let d = decodifica(&c, s).unwrap();
        let mut e = [0i64; 6];
        for (a, b) in img.rgb.chunks_exact(3).zip(d.rgb.chunks_exact(3)) {
            for i in 0..3 {
                e[i] += (a[i] as i64 - b[i] as i64).abs();
            }
            let (y1, cb1, cr1) = ycbcr(a[0], a[1], a[2]);
            let (y2, cb2, cr2) = ycbcr(b[0], b[1], b[2]);
            e[3] += (y1 as i64 - y2 as i64).abs();
            e[4] += (cb1 as i64 - cb2 as i64).abs();
            e[5] += (cr1 as i64 - cr2 as i64).abs();
        }
        let n = (w * h) as i64;
        println!("{nome:<16} PSNR {:?}  err medio R {} G {} B {} | Y {} Cb {} Cr {}  (dettaglio {} byte)", psnr_x100(&img, &d), e[0] / n, e[1] / n, e[2] / n, e[3] / n, e[4] / n, e[5] / n, c.dettaglio.len());
    }
    // per cella: pixel, errore medio e massimo della luce dopo colore + pendenza (ricalcolati qui dagli strati)
    let mappa = g.mappa();
    let bari = g.baricentri(&mappa);
    let n = g.celle();
    let mut st = vec![(0i64, 0i64, 0i64, 0i64); n]; // n, somma |err|, max |err|, somma err
    for (k, &cel) in mappa.iter().enumerate() {
        let cel = cel as usize;
        let (cx, cy) = bari[cel];
        let dx = ((k % w as usize) as i64 * 256 + 128 - cx) / 256;
        let dy = ((k / w as usize) as i64 * 256 + 128 - cy) / 256;
        let gx = c.pendenza[cel * 6] as i8 as i64;
        let gy = c.pendenza[cel * 6 + 1] as i8 as i64;
        let prev = c.colore[cel * 3] as i64 + (gx * dx + gy * dy) / 32;
        let px = &img.rgb[k * 3..k * 3 + 3];
        let y = ycbcr(px[0], px[1], px[2]).0 as i64;
        let e = y - prev;
        st[cel].0 += 1;
        st[cel].1 += e.abs();
        st[cel].2 = st[cel].2.max(e.abs());
        st[cel].3 += e;
    }
    for (cel, s) in st.iter().enumerate().filter(|(_, s)| s.0 > 0) {
        let (cx, cy) = g.centro(cel as u32);
        println!("cella {cel:>2} centro ({:>4},{:>4}) px {:>5} err medio {} max {} somma {} pend {:?}", cx / 256, cy / 256, s.0, s.1 / s.0, s.2, s.3, &c.pendenza[cel * 6..cel * 6 + 2].iter().map(|&v| v as i8).collect::<Vec<_>>());
    }
    // le pendenze di qualche cella interna
    for cel in [g.celle() / 2, g.celle() / 2 + 1] {
        let p: Vec<i8> = c.pendenza[cel * 6..cel * 6 + 6].iter().map(|&v| v as i8).collect();
        println!("cella {cel}: colore {:?} pendenze {p:?}", &c.colore[cel * 3..cel * 3 + 3]);
    }
}

#[allow(dead_code)]
fn _niente() {}
