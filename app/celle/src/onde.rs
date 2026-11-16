//! Trasformata a onde intera 5/3 (quella "reversibile" di JPEG 2000) con il metodo del sollevamento: una differenza
//! dal vicino medio e una media corretta. Solo somme e spostamenti; si inverte esattamente. In due dimensioni per
//! livelli: a ogni livello la parte "bassa" (la media) resta in alto a sinistra e si trasforma di nuovo, le tre
//! bande "alte" (dettaglio orizzontale, verticale, diagonale) restano dove sono.
//!
//! Disposizione in memoria dopo la trasformata, per un piano `w × h` e livello `l` (1 = il più fine):
//! `w_l = ⌈w_{l-1}/2⌉`, la banda bassa occupa `[0,w_l)×[0,h_l)`, HL `[w_l,w_{l-1})×[0,h_l)`, LH `[0,w_l)×[h_l,h_{l-1})`,
//! HH `[w_l,w_{l-1})×[h_l,h_{l-1})`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orient {
    /// alta in orizzontale (bordi verticali)
    HL,
    /// alta in verticale (bordi orizzontali)
    LH,
    /// alta in entrambe (diagonali)
    HH,
    /// la media che resta all'ultimo livello
    LL,
}

impl Orient {
    pub fn indice(self) -> usize {
        match self {
            Orient::HL => 0,
            Orient::LH => 1,
            Orient::HH => 2,
            Orient::LL => 3,
        }
    }
}

/// Una banda: rettangolo dentro il piano trasformato.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Banda {
    pub x0: usize,
    pub y0: usize,
    pub w: usize,
    pub h: usize,
    /// 1 = la più fine; per LL vale il numero dei livelli
    pub livello: u8,
    pub orient: Orient,
}

/// Dimensioni della parte bassa dopo `l` livelli.
pub fn dimensioni(w: usize, h: usize, l: u8) -> (usize, usize) {
    let (mut w, mut h) = (w, h);
    for _ in 0..l {
        w = w.div_ceil(2);
        h = h.div_ceil(2);
    }
    (w, h)
}

/// Le bande di un piano `w × h` con `livelli` livelli, dalla più grossolana alla più fine: LL, poi per ogni livello
/// dal più alto al più basso HL, LH, HH.
pub fn bande(w: usize, h: usize, livelli: u8) -> Vec<Banda> {
    let mut v = Vec::with_capacity(1 + 3 * livelli as usize);
    let (wl, hl) = dimensioni(w, h, livelli);
    v.push(Banda { x0: 0, y0: 0, w: wl, h: hl, livello: livelli, orient: Orient::LL });
    for l in (1..=livelli).rev() {
        let (wp, hp) = dimensioni(w, h, l - 1);
        let (wl, hl) = (wp.div_ceil(2), hp.div_ceil(2));
        v.push(Banda { x0: wl, y0: 0, w: wp - wl, h: hl, livello: l, orient: Orient::HL });
        v.push(Banda { x0: 0, y0: hl, w: wl, h: hp - hl, livello: l, orient: Orient::LH });
        v.push(Banda { x0: wl, y0: hl, w: wp - wl, h: hp - hl, livello: l, orient: Orient::HH });
    }
    v
}

/// Un passo di sollevamento su una riga: dentro `x` restano prima le medie (⌈n/2⌉) poi le differenze (⌊n/2⌋).
fn avanti_1d(x: &mut [i32], tmp: &mut Vec<i32>) {
    let n = x.len();
    if n < 2 {
        return;
    }
    let (nl, nh) = (n.div_ceil(2), n / 2);
    tmp.clear();
    tmp.resize(n, 0);
    for i in 0..nh {
        let a = x[2 * i];
        let b = if 2 * i + 2 < n { x[2 * i + 2] } else { a };
        tmp[nl + i] = x[2 * i + 1] - ((a + b) >> 1);
    }
    for i in 0..nl {
        let dl = if i == 0 { tmp[nl] } else { tmp[nl + i - 1] };
        let dr = if i < nh { tmp[nl + i] } else { tmp[nl + i - 1] };
        tmp[i] = x[2 * i] + ((dl + dr + 2) >> 2);
    }
    x.copy_from_slice(tmp);
}

fn indietro_1d(x: &mut [i32], tmp: &mut Vec<i32>) {
    let n = x.len();
    if n < 2 {
        return;
    }
    let (nl, nh) = (n.div_ceil(2), n / 2);
    tmp.clear();
    tmp.resize(n, 0);
    for i in 0..nl {
        let dl = if i == 0 { x[nl] } else { x[nl + i - 1] };
        let dr = if i < nh { x[nl + i] } else { x[nl + i - 1] };
        tmp[2 * i] = x[i] - ((dl + dr + 2) >> 2);
    }
    for i in 0..nh {
        let a = tmp[2 * i];
        let b = if 2 * i + 2 < n { tmp[2 * i + 2] } else { a };
        tmp[2 * i + 1] = x[nl + i] + ((a + b) >> 1);
    }
    x.copy_from_slice(tmp);
}

/// Trasformata in avanti sul posto, `livelli` livelli.
pub fn avanti(p: &mut [i32], w: usize, h: usize, livelli: u8) {
    debug_assert_eq!(p.len(), w * h);
    let mut tmp = Vec::new();
    let mut col = Vec::new();
    let (mut wl, mut hl) = (w, h);
    for _ in 0..livelli {
        for y in 0..hl {
            avanti_1d(&mut p[y * w..y * w + wl], &mut tmp);
        }
        col.clear();
        col.resize(hl, 0);
        for x in 0..wl {
            for y in 0..hl {
                col[y] = p[y * w + x];
            }
            avanti_1d(&mut col, &mut tmp);
            for y in 0..hl {
                p[y * w + x] = col[y];
            }
        }
        wl = wl.div_ceil(2);
        hl = hl.div_ceil(2);
    }
}

/// Trasformata all'indietro sul posto.
pub fn indietro(p: &mut [i32], w: usize, h: usize, livelli: u8) {
    debug_assert_eq!(p.len(), w * h);
    let mut tmp = Vec::new();
    let mut col = Vec::new();
    for l in (0..livelli).rev() {
        let (wl, hl) = dimensioni(w, h, l);
        col.clear();
        col.resize(hl, 0);
        for x in 0..wl {
            for y in 0..hl {
                col[y] = p[y * w + x];
            }
            indietro_1d(&mut col, &mut tmp);
            for y in 0..hl {
                p[y * w + x] = col[y];
            }
        }
        for y in 0..hl {
            indietro_1d(&mut p[y * w..y * w + wl], &mut tmp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caso(stato: &mut u64) -> u64 {
        *stato ^= *stato << 13;
        *stato ^= *stato >> 7;
        *stato ^= *stato << 17;
        *stato
    }

    #[test]
    fn avanti_e_indietro_tornano_uguali_anche_con_misure_dispari() {
        let mut stato = 12345u64;
        for &(w, h, l) in &[(1usize, 1usize, 3u8), (2, 1, 1), (1, 7, 2), (7, 5, 3), (8, 8, 3), (33, 17, 5), (100, 70, 4), (3, 3, 1)] {
            let orig: Vec<i32> = (0..w * h).map(|_| (caso(&mut stato) % 256) as i32).collect();
            let mut p = orig.clone();
            avanti(&mut p, w, h, l);
            indietro(&mut p, w, h, l);
            assert_eq!(p, orig, "{w}x{h} livelli {l}");
        }
    }

    #[test]
    fn un_piano_costante_ha_solo_la_media() {
        let (w, h) = (16, 12);
        let mut p = vec![77i32; w * h];
        avanti(&mut p, w, h, 2);
        let b = bande(w, h, 2);
        assert_eq!(b.len(), 7);
        let ll = b[0];
        assert_eq!((ll.w, ll.h), (4, 3));
        for banda in &b[1..] {
            for y in 0..banda.h {
                for x in 0..banda.w {
                    assert_eq!(p[(banda.y0 + y) * w + banda.x0 + x], 0, "{banda:?}");
                }
            }
        }
        for y in 0..ll.h {
            for x in 0..ll.w {
                assert_eq!(p[y * w + x], 77);
            }
        }
    }

    #[test]
    fn le_bande_coprono_tutto_il_piano_senza_sovrapporsi() {
        for &(w, h, l) in &[(7usize, 5usize, 3u8), (8, 8, 1), (33, 17, 4), (1, 9, 2)] {
            let mut conta = vec![0u8; w * h];
            for b in bande(w, h, l) {
                for y in 0..b.h {
                    for x in 0..b.w {
                        conta[(b.y0 + y) * w + b.x0 + x] += 1;
                    }
                }
            }
            assert!(conta.iter().all(|&c| c == 1), "{w}x{h} livelli {l}");
        }
    }
}
