//! BMP a 24 bit senza compressione: il formato più semplice che c'è, per leggere e scrivere immagini senza
//! dipendenze (misure, prove, fotogrammi dei reel). Non è il formato del codec.

use crate::{Errore, Immagine};

/// Legge un BMP a 24 bit (righe dal basso o dall'alto).
pub fn leggi(b: &[u8]) -> Result<Immagine, Errore> {
    if b.len() < 54 || &b[0..2] != b"BM" {
        return Err(Errore::Immagine("non è un BMP"));
    }
    let inizio = u32::from_le_bytes([b[10], b[11], b[12], b[13]]) as usize;
    let w = i32::from_le_bytes([b[18], b[19], b[20], b[21]]);
    let h = i32::from_le_bytes([b[22], b[23], b[24], b[25]]);
    let bpp = u16::from_le_bytes([b[28], b[29]]);
    let compressione = u32::from_le_bytes([b[30], b[31], b[32], b[33]]);
    if bpp != 24 || compressione != 0 {
        return Err(Errore::Immagine("serve un BMP a 24 bit senza compressione"));
    }
    if w <= 0 || h == 0 {
        return Err(Errore::Immagine("dimensioni"));
    }
    let (w, dal_basso, h) = (w as usize, h > 0, h.unsigned_abs() as usize);
    let riga = (w * 3).div_ceil(4) * 4;
    if inizio + riga * h > b.len() {
        return Err(Errore::Immagine("BMP troncato"));
    }
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        let yy = if dal_basso { h - 1 - y } else { y };
        let r = &b[inizio + yy * riga..inizio + yy * riga + w * 3];
        for px in r.chunks_exact(3) {
            rgb.extend_from_slice(&[px[2], px[1], px[0]]);
        }
    }
    Ok(Immagine { larghezza: w as u32, altezza: h as u32, rgb })
}

/// Scrive un BMP a 24 bit (righe dal basso, come fanno tutti).
pub fn scrivi(img: &Immagine) -> Vec<u8> {
    let (w, h) = (img.larghezza as usize, img.altezza as usize);
    let riga = (w * 3).div_ceil(4) * 4;
    let dati = riga * h;
    let mut b = Vec::with_capacity(54 + dati);
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&((54 + dati) as u32).to_le_bytes());
    b.extend_from_slice(&[0, 0, 0, 0]);
    b.extend_from_slice(&54u32.to_le_bytes());
    b.extend_from_slice(&40u32.to_le_bytes());
    b.extend_from_slice(&(w as i32).to_le_bytes());
    b.extend_from_slice(&(h as i32).to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&24u16.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&(dati as u32).to_le_bytes());
    b.extend_from_slice(&2835u32.to_le_bytes());
    b.extend_from_slice(&2835u32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    for y in (0..h).rev() {
        let r = &img.rgb[y * w * 3..(y + 1) * w * 3];
        for px in r.chunks_exact(3) {
            b.extend_from_slice(&[px[2], px[1], px[0]]);
        }
        b.resize(b.len() + riga - w * 3, 0);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrivi_e_rileggi() {
        let img = Immagine { larghezza: 3, altezza: 2, rgb: (0..18).map(|v| v as u8 * 13).collect() };
        let b = scrivi(&img);
        assert_eq!(b.len(), 54 + 12 * 2);
        assert_eq!(leggi(&b).unwrap(), img);
        assert!(leggi(&b[..40]).is_err());
        assert!(leggi(&[]).is_err());
    }
}
