//! Strati 0 e 1 compressi: il colore e la pendenza di ogni cella si scrivono come differenza da una previsione
//! (le celle vicine) con la codifica aritmetica di `aritmetica`, usando la stessa scrittura dei valori del
//! dettaglio. Una cella base è prevista dalla media di quella a sinistra e di quella sopra; le sei celle di una
//! cella raffinata sono previste dal centro della loro cella. Nessuna geometria in più: il decodificatore conosce già
//! la griglia.

use crate::aritmetica::{Lettore, Scrittore};
use crate::dettaglio::{leggi_valore, scrivi_valore, secchio, Contesti, Dove};
use crate::Griglia;

/// Per ogni cella base in ordine di riga: (prima cella finale, cella a sinistra, cella sopra, quante celle: 1 o 7).
pub(crate) fn ordine(g: &Griglia) -> Vec<(usize, Option<usize>, Option<usize>, usize)> {
    let (colonne, righe) = (g.colonne as usize, g.righe as usize);
    let mut v = Vec::with_capacity(colonne * righe);
    for j in 0..righe {
        for i in 0..colonne {
            let k = j * colonne + i;
            let c0 = g.inizio[k] as usize;
            let sinistra = (i > 0).then(|| g.inizio[k - 1] as usize);
            let sopra = (j > 0).then(|| g.inizio[k - colonne] as usize);
            v.push((c0, sinistra, sopra, if g.raffinate[k] { 7 } else { 1 }));
        }
    }
    v
}

/// Scrive (o legge) `per_cella` valori per ogni cella, previsti dalle vicine. `valori` è `celle × per_cella`,
/// in ordine di cella finale. Il contesto: canale (o), tipo di cella (l), grandezza dell'ultimo resto (vic).
pub(crate) fn codifica_valori(g: &Griglia, valori: &[i32], per_cella: usize) -> Vec<u8> {
    let mut sc = Scrittore::nuovo();
    let mut ctx = Contesti::nuovi();
    let mut ultimo = vec![0i64; per_cella];
    for (c0, sinistra, sopra, quante) in ordine(g) {
        for q in 0..per_cella {
            let prev = previsione(valori, per_cella, q, sinistra, sopra);
            let resto = valori[c0 * per_cella + q] - prev;
            let d = Dove { o: (q / 2).min(3), l: (q % 2).min(1), vic: secchio(ultimo[q]), gen: 0, sl: 0, su: 0 };
            scrivi_valore(&mut sc, &mut ctx, &d, resto);
            ultimo[q] = resto.unsigned_abs() as i64;
        }
        for c in c0 + 1..c0 + quante {
            for q in 0..per_cella {
                let resto = valori[c * per_cella + q] - valori[c0 * per_cella + q];
                let d = Dove { o: (q / 2).min(3), l: 2 + (q % 2).min(1), vic: secchio(ultimo[q]), gen: 1, sl: 0, su: 0 };
                scrivi_valore(&mut sc, &mut ctx, &d, resto);
                ultimo[q] = resto.unsigned_abs() as i64;
            }
        }
    }
    sc.chiudi()
}

pub(crate) fn decodifica_valori(g: &Griglia, b: &[u8], per_cella: usize, minimo: i32, massimo: i32) -> Vec<i32> {
    let mut le = Lettore::nuovo(b);
    let mut ctx = Contesti::nuovi();
    let mut valori = vec![0i32; g.celle() * per_cella];
    let mut ultimo = vec![0i64; per_cella];
    for (c0, sinistra, sopra, quante) in ordine(g) {
        for q in 0..per_cella {
            let prev = previsione(&valori, per_cella, q, sinistra, sopra);
            let d = Dove { o: (q / 2).min(3), l: (q % 2).min(1), vic: secchio(ultimo[q]), gen: 0, sl: 0, su: 0 };
            let resto = leggi_valore(&mut le, &mut ctx, &d);
            valori[c0 * per_cella + q] = prev.saturating_add(resto).clamp(minimo, massimo);
            ultimo[q] = resto.unsigned_abs() as i64;
        }
        for c in c0 + 1..c0 + quante {
            for q in 0..per_cella {
                let d = Dove { o: (q / 2).min(3), l: 2 + (q % 2).min(1), vic: secchio(ultimo[q]), gen: 1, sl: 0, su: 0 };
                let resto = leggi_valore(&mut le, &mut ctx, &d);
                valori[c * per_cella + q] = valori[c0 * per_cella + q].saturating_add(resto).clamp(minimo, massimo);
                ultimo[q] = resto.unsigned_abs() as i64;
            }
        }
    }
    valori
}

#[inline]
pub(crate) fn previsione(valori: &[i32], per_cella: usize, q: usize, sinistra: Option<usize>, sopra: Option<usize>) -> i32 {

    match (sinistra, sopra) {
        (Some(s), Some(u)) => (valori[s * per_cella + q] + valori[u * per_cella + q] + 1) >> 1,
        (Some(s), None) => valori[s * per_cella + q],
        (None, Some(u)) => valori[u * per_cella + q],
        (None, None) => 0,
    }
}

/// Strato 0: Y, Cb, Cr per cella (3 byte ciascuna) → byte compressi.
pub(crate) fn codifica_colore(g: &Griglia, colore: &[u8]) -> Vec<u8> {
    let v: Vec<i32> = colore.iter().map(|&b| b as i32).collect();
    codifica_valori(g, &v, 3)
}

/// Strato 0 compresso → Y, Cb, Cr per cella.
pub(crate) fn decodifica_colore(g: &Griglia, b: &[u8]) -> Vec<u8> {
    decodifica_valori(g, b, 3, 0, 255).into_iter().map(|v| v as u8).collect()
}

/// Strato 1: sei pendenze (i8) per cella → byte compressi.
pub(crate) fn codifica_pendenza(g: &Griglia, pendenza: &[u8]) -> Vec<u8> {
    let v: Vec<i32> = pendenza.iter().map(|&b| b as i8 as i32).collect();
    codifica_valori(g, &v, 6)
}

/// Strato 1 compresso → sei pendenze (i8 come u8) per cella.
pub(crate) fn decodifica_pendenza(g: &Griglia, b: &[u8]) -> Vec<u8> {
    decodifica_valori(g, b, 6, -127, 127).into_iter().map(|v| v as i8 as u8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colore_e_pendenza_tornano_uguali_e_pesano_meno() {
        let mut g = Griglia::nuova(300, 200, 12);
        let mut raff = vec![false; g.celle_base()];
        for (i, r) in raff.iter_mut().enumerate() {
            *r = i % 5 == 0;
        }
        g = g.con_raffinate(raff);
        let n = g.celle();
        // colori lisci (gradiente) con un po' di rumore deterministico, pendenze piccole
        let mut colore = Vec::with_capacity(n * 3);
        let mut pend = Vec::with_capacity(n * 6);
        for c in 0..n {
            let (x, y) = g.centro(c as u32);
            colore.push(((x / 256) * 255 / 300).clamp(0, 255) as u8);
            colore.push(((y / 256) * 255 / 200).clamp(0, 255) as u8);
            colore.push((128 + (c % 7) as i64 - 3) as u8);
            for q in 0..6 {
                pend.push((((c * 31 + q * 17) % 9) as i32 - 4) as i8 as u8);
            }
        }
        let bc = codifica_colore(&g, &colore);
        let bp = codifica_pendenza(&g, &pend);
        assert_eq!(decodifica_colore(&g, &bc), colore);
        assert_eq!(decodifica_pendenza(&g, &bp), pend);
        assert!(bc.len() * 2 < colore.len(), "colore: {} → {} byte", colore.len(), bc.len());
        assert!(bp.len() * 2 < pend.len(), "pendenza: {} → {} byte", pend.len(), bp.len());
        // byte a caso o vuoti: nessun panico, lunghezze giuste
        assert_eq!(decodifica_colore(&g, &[]).len(), n * 3);
        assert_eq!(decodifica_pendenza(&g, &[7, 200, 13, 99, 0, 255]).len(), n * 6);
    }
}
