//! Reel: fotogrammi uno dopo l'altro sulla stessa griglia ad alveare.
//!
//! Il primo fotogramma (e uno ogni tanto) è una **chiave**: una foto a celle come in `lib.rs`, con i suoi tre
//! strati. Gli altri sono **previsti**: per ogni cella si cerca di quanto si è spostata rispetto al fotogramma
//! prima (uno spostamento `(dx, dy)` per cella in mezzi pixel, solo somme di differenze assolute e medie), il fotogramma prima
//! spostato cella per cella fa da base, e il resto passa per la stessa trasformata a onde e la stessa codifica
//! aritmetica del dettaglio delle foto (`dettaglio`, con `Base::Tutto`: qui la base somiglia davvero all'immagine,
//! bande alte comprese). Gli spostamenti viaggiano compressi come gli strati 0 e 1 (`strati`: previsti dalle celle
//! vicine). La griglia è quella della chiave: non si trasmette di nuovo.
//!
//! L'alveare al lavoro sul video: dentro ogni fotogramma previsto il resto arriva a pezzi, cella per cella, con le
//! celle in ordine di quanto la previsione ha sbagliato (`dettaglio::percorri`): prima quelle che si muovono o si
//! scoprono, per ultime quelle ferme. Un fotogramma previsto troncato è intero (la base c'è sempre), solo meno fine
//! dove la banda non è arrivata.
//!
//! Chi codifica tiene come riferimento la **propria ricostruzione** (quella che vede chi legge con tutti i pezzi):
//! chi legge un reel a pezzi troncati si allontana un po' a ogni fotogramma, fino alla chiave dopo.

use crate::dettaglio::{self, Base, Piani, Piano, LIVELLI_MAX};
use crate::{codifica, decodifica_piani, leggi_varint, piani_di, piani_in_rgb, scrivi_varint, strati, ycbcr, Codificata, Errore, Griglia, Immagine, Parametri, Strati};

/// Versione del formato del reel.
pub const VERSIONE_REEL: u8 = 1;
const MAGIA_REEL: &[u8; 4] = b"M4RL";
/// Spostamento massimo per cella che il formato sa scrivere (pixel).
pub const CERCA_MAX: u8 = 32;

/// Parametri di un reel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParametriReel {
    /// Parametri dei fotogrammi chiave; passo, passo dei colori e livelli valgono anche per il resto dei previsti.
    pub foto: Parametri,
    /// Una chiave ogni tanti fotogrammi (0 = solo la prima).
    pub chiave_ogni: u16,
    /// Raggio di ricerca dello spostamento per cella, in pixel (0 = niente spostamento, solo la differenza).
    pub cerca: u8,
    /// Se dopo la ricerca lo scarto medio per pixel della luce supera questa soglia (cambio di scena), il fotogramma
    /// diventa una chiave (0 = mai).
    pub soglia_chiave: u8,
}

impl Default for ParametriReel {
    fn default() -> Self {
        Self { foto: Parametri::default(), chiave_ogni: 60, cerca: 16, soglia_chiave: 24 }
    }
}

/// Un fotogramma previsto dal precedente: tre pezzi, ognuno può essere un blob.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Previsto {
    /// passo, passo dei colori, livelli, raggio di ricerca
    pub testa: Vec<u8>,
    /// spostamento (dx, dy) per cella, compresso
    pub vettori: Vec<u8>,
    /// il resto, a pezzi cella per cella
    pub dettaglio: Vec<u8>,
}

impl Previsto {
    pub fn byte_totali(&self) -> usize {
        self.testa.len() + self.vettori.len() + self.dettaglio.len()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fotogramma {
    Chiave(Codificata),
    Previsto(Previsto),
}

impl Fotogramma {
    pub fn byte_totali(&self) -> usize {
        match self {
            Fotogramma::Chiave(c) => c.byte_totali(),
            Fotogramma::Previsto(p) => p.byte_totali(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Reel {
    pub fotogrammi: Vec<Fotogramma>,
}

/// Quello che resta da un fotogramma all'altro, uguale da una parte e dall'altra.
struct Stato {
    griglia: Griglia,
    mappa: Vec<u32>,
    /// gli indici dei pixel di ogni cella
    pixel: Vec<Vec<u32>>,
    /// il fotogramma prima, ricostruito (luce intera, colori dimezzati)
    rif: Piani,
    /// gli spostamenti del fotogramma prima, per cella
    vettori: Vec<(i32, i32)>,
}

impl Stato {
    fn nuovo(griglia: Griglia, rif: Piani) -> Self {
        let mappa = griglia.mappa();
        let n = griglia.celle();
        let mut pixel = vec![Vec::new(); n];
        for (k, &c) in mappa.iter().enumerate() {
            pixel[c as usize].push(k as u32);
        }
        Stato { griglia, mappa, pixel, rif, vettori: vec![(0, 0); n] }
    }
}

// ─── Spostamento per cella ──────────────────────────────────────────────────
//
// Gli spostamenti sono in **mezzi pixel** (come i codec a pixel: tra due pixel si prende la media, aritmetica
// intera). Sulla luce intera un vettore `(hx, hy)` sposta di `hx/2` pixel; sui colori dimezzati di `hx/4`: un quarto
// di pixel, con la media pesata dei quattro pixel intorno.

/// Il pixel della luce in `(x + hx/2, y + hy/2)`: tra due pixel la media, tra quattro la media dei quattro. Ai
/// bordi si prende il pixel più vicino.
#[inline]
fn campione(rif: &Piano, x: i32, y: i32, hx: i32, hy: i32) -> i32 {
    let (w, h) = (rif.w as i32, rif.h as i32);
    let (ix, iy) = (x + hx.div_euclid(2), y + hy.div_euclid(2));
    let at = |xx: i32, yy: i32| rif.v[yy.clamp(0, h - 1) as usize * rif.w + xx.clamp(0, w - 1) as usize];
    match (hx & 1, hy & 1) {
        (0, 0) => at(ix, iy),
        (1, 0) => (at(ix, iy) + at(ix + 1, iy) + 1) >> 1,
        (0, _) => (at(ix, iy) + at(ix, iy + 1) + 1) >> 1,
        _ => (at(ix, iy) + at(ix + 1, iy) + at(ix, iy + 1) + at(ix + 1, iy + 1) + 2) >> 2,
    }
}

/// Il pixel di un colore dimezzato in `(x + qx/4, y + qy/4)`: media pesata dei quattro pixel intorno.
#[inline]
fn campione_quarto(rif: &Piano, x: i32, y: i32, qx: i32, qy: i32) -> i32 {
    let (w, h) = (rif.w as i32, rif.h as i32);
    let (ix, iy) = (x + qx.div_euclid(4), y + qy.div_euclid(4));
    let (fx, fy) = (qx.rem_euclid(4), qy.rem_euclid(4));
    let at = |xx: i32, yy: i32| rif.v[yy.clamp(0, h - 1) as usize * rif.w + xx.clamp(0, w - 1) as usize];
    if fx == 0 && fy == 0 {
        return at(ix, iy);
    }
    ((4 - fx) * (4 - fy) * at(ix, iy) + fx * (4 - fy) * at(ix + 1, iy) + (4 - fx) * fy * at(ix, iy + 1) + fx * fy * at(ix + 1, iy + 1) + 8) >> 4
}

/// Somma delle differenze assolute della luce della cella con il riferimento spostato di `(hx, hy)` mezzi pixel.
/// `ogni` > 1 usa un pixel ogni tanti: per la ricerca grossolana.
fn scarto(cur: &Piano, rif: &Piano, pix: &[u32], hx: i32, hy: i32, ogni: usize) -> i64 {
    let (w, h) = (cur.w as i32, cur.h as i32);
    let mut s = 0i64;
    let mut i = 0;
    if hx & 1 == 0 && hy & 1 == 0 {
        let (dx, dy) = (hx / 2, hy / 2);
        while i < pix.len() {
            let k = pix[i] as usize;
            let (x, y) = ((k % cur.w) as i32, (k / cur.w) as i32);
            let (xx, yy) = ((x + dx).clamp(0, w - 1), (y + dy).clamp(0, h - 1));
            s += (cur.v[k] - rif.v[yy as usize * cur.w + xx as usize]).unsigned_abs() as i64;
            i += ogni;
        }
    } else {
        while i < pix.len() {
            let k = pix[i] as usize;
            s += (cur.v[k] - campione(rif, (k % cur.w) as i32, (k / cur.w) as i32, hx, hy)).unsigned_abs() as i64;
            i += ogni;
        }
    }
    s
}

/// Lo spostamento di ogni cella, in mezzi pixel: si provano i candidati (fermo, quello previsto dalle celle vicine,
/// quello della stessa cella nel fotogramma prima, quelli delle celle a sinistra e sopra), se nessuno spiega la
/// cella si cerca a passi di 4 pixel su pochi pixel, poi si raffina di un pixel alla volta e infine di mezzo pixel
/// finché migliora. Il costo è lo scarto più una piccola penale per gli spostamenti lontani da quello previsto
/// (costano byte e fanno bordi). Restituisce gli spostamenti e lo scarto totale della luce.
fn cerca_vettori(cur: &Piano, rif: &Piano, st: &Stato, cerca: i32, passo: u8) -> (Vec<(i32, i32)>, i64) {
    let n = st.griglia.celle();
    let mut v = vec![(0i32, 0i32); n];
    let mut piatti = vec![0i32; n * 2];
    let mut tot = 0i64;
    let lambda = passo.max(1) as i64;
    let lim = 2 * cerca;
    for (c0, sinistra, sopra, quante) in strati::ordine(&st.griglia) {
        for c in c0..c0 + quante {
            let pix = &st.pixel[c];
            if pix.is_empty() || cerca == 0 {
                tot += scarto(cur, rif, pix, 0, 0, 1);
                continue;
            }
            let (px, py) = if c == c0 { (strati::previsione(&piatti, 2, 0, sinistra, sopra), strati::previsione(&piatti, 2, 1, sinistra, sopra)) } else { v[c0] };
            let costo = |hx: i32, hy: i32| scarto(cur, rif, pix, hx, hy, 1) + lambda * ((hx - px).abs() + (hy - py).abs()) as i64;
            let mut candidati = vec![(0, 0), (px, py), st.vettori[c]];
            if let Some(s) = sinistra {
                candidati.push(v[s]);
            }
            if let Some(u) = sopra {
                candidati.push(v[u]);
            }
            let mut migliore = (i64::MAX, (0, 0));
            for (hx, hy) in candidati {
                let (hx, hy) = (hx.clamp(-lim, lim), hy.clamp(-lim, lim));
                let k = costo(hx, hy);
                if k < migliore.0 {
                    migliore = (k, (hx, hy));
                }
            }
            // niente spiega la cella: ricerca grossolana a passi di 4 pixel su un pixel ogni 3
            if cerca >= 4 && migliore.0 > 4 * pix.len() as i64 {
                let ogni = 3usize;
                let mut grossa = (i64::MAX, migliore.1);
                let mut hy = -lim;
                while hy <= lim {
                    let mut hx = -lim;
                    while hx <= lim {
                        let k = scarto(cur, rif, pix, hx, hy, ogni) * ogni as i64 + lambda * ((hx - px).abs() + (hy - py).abs()) as i64;
                        if k < grossa.0 {
                            grossa = (k, (hx, hy));
                        }
                        hx += 8;
                    }
                    hy += 8;
                }
                let k = costo(grossa.1 .0, grossa.1 .1);
                if k < migliore.0 {
                    migliore = (k, grossa.1);
                }
            }
            // un pixel alla volta, poi mezzo pixel alla volta, finché migliora
            for salto in [2, 1] {
                loop {
                    let (bx, by) = migliore.1;
                    let mut meglio = false;
                    for (ddx, ddy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                        let (hx, hy) = (bx + ddx * salto, by + ddy * salto);
                        if hx.abs() > lim || hy.abs() > lim {
                            continue;
                        }
                        let k = costo(hx, hy);
                        if k < migliore.0 {
                            migliore = (k, (hx, hy));
                            meglio = true;
                        }
                    }
                    if !meglio {
                        break;
                    }
                }
            }
            v[c] = migliore.1;
            piatti[c * 2] = migliore.1 .0;
            piatti[c * 2 + 1] = migliore.1 .1;
            tot += scarto(cur, rif, pix, migliore.1 .0, migliore.1 .1, 1);
        }
    }
    (v, tot)
}

/// Il fotogramma prima spostato cella per cella (mezzi pixel sulla luce, quarti sui colori dimezzati): la base del
/// fotogramma previsto.
fn prevedi(rif: &Piani, mappa: &[u32], v: &[(i32, i32)]) -> Piani {
    let (w, h) = (rif.y.w, rif.y.h);
    let mut y = Piano::nuovo(w, h);
    for k in 0..w * h {
        let (hx, hy) = v[mappa[k] as usize];
        y.v[k] = campione(&rif.y, (k % w) as i32, (k / w) as i32, hx, hy);
    }
    let (wc, hc) = (rif.cb.w, rif.cb.h);
    let mut cb = Piano::nuovo(wc, hc);
    let mut cr = Piano::nuovo(wc, hc);
    for yc in 0..hc {
        for xc in 0..wc {
            let k = (2 * yc).min(h - 1) * w + (2 * xc).min(w - 1);
            let (hx, hy) = v[mappa[k] as usize];
            cb.v[yc * wc + xc] = campione_quarto(&rif.cb, xc as i32, yc as i32, hx, hy);
            cr.v[yc * wc + xc] = campione_quarto(&rif.cr, xc as i32, yc as i32, hx, hy);
        }
    }
    Piani { y, cb, cr }
}

// ─── Codifica e decodifica ──────────────────────────────────────────────────

/// Codifica una sequenza di fotogrammi della stessa misura.
pub fn codifica_reel(fotogrammi: &[Immagine], p: ParametriReel) -> Result<Reel, Errore> {
    let primo = fotogrammi.first().ok_or(Errore::Immagine("nessun fotogramma"))?;
    let (w, h) = (primo.larghezza as usize, primo.altezza as usize);
    let cerca = p.cerca.min(CERCA_MAX) as i32;
    let passo = p.foto.passo_dettaglio;
    let livelli = p.foto.livelli.clamp(1, LIVELLI_MAX);
    let mut stato: Option<Stato> = None;
    let mut out = Vec::with_capacity(fotogrammi.len());
    for (i, img) in fotogrammi.iter().enumerate() {
        if img.larghezza != primo.larghezza || img.altezza != primo.altezza || img.rgb.len() != w * h * 3 {
            return Err(Errore::Immagine("fotogrammi di misure diverse"));
        }
        let luce: Vec<u8> = img.rgb.chunks_exact(3).map(|px| ycbcr(px[0], px[1], px[2]).0).collect();
        let orig = piani_di(img, &luce);
        let mut vettori = None;
        if let Some(st) = stato.as_ref() {
            if !(p.chiave_ogni > 0 && i % p.chiave_ogni as usize == 0) {
                let (v, tot) = cerca_vettori(&orig.y, &st.rif.y, st, cerca, passo);
                if p.soglia_chiave == 0 || tot / (w * h) as i64 <= p.soglia_chiave as i64 {
                    vettori = Some(v);
                }
            }
        }
        match vettori {
            Some(v) => {
                let st = stato.as_mut().expect("stato");
                let n = st.griglia.celle();
                let pred = prevedi(&st.rif, &st.mappa, &v);
                let dettaglio = if passo > 0 { dettaglio::codifica(&orig, &pred, passo, p.foto.passo_croma, livelli, &st.mappa, n, Base::Tutto) } else { vec![] };
                // il riferimento per il fotogramma dopo è quello che vede chi legge, non l'originale
                let fine = if passo > 0 { dettaglio::decodifica(&dettaglio, &pred, passo, p.foto.passo_croma, livelli, &st.mappa, n, Base::Tutto) } else { pred };
                let piatti: Vec<i32> = v.iter().flat_map(|&(a, b)| [a, b]).collect();
                let vettori = strati::codifica_valori(&st.griglia, &piatti, 2);
                st.rif = fine;
                st.vettori = v;
                out.push(Fotogramma::Previsto(Previsto { testa: vec![passo, p.foto.passo_croma, livelli, cerca as u8], vettori, dettaglio }));
            }
            None => {
                let (cod, griglia) = codifica(img, p.foto)?;
                let (rif, _) = decodifica_piani(&cod, Strati::Tutti)?;
                stato = Some(Stato::nuovo(griglia, rif));
                out.push(Fotogramma::Chiave(cod));
            }
        }
    }
    Ok(Reel { fotogrammi: out })
}

/// Gli spostamenti di un fotogramma previsto, per cella.
fn leggi_vettori(st: &Stato, p: &Previsto) -> Result<(u8, u8, u8, Vec<(i32, i32)>), Errore> {
    if p.testa.len() < 4 {
        return Err(Errore::Formato("intestazione del fotogramma previsto"));
    }
    let (passo, croma, livelli, cerca) = (p.testa[0], p.testa[1], p.testa[2], p.testa[3]);
    if !(1..=LIVELLI_MAX).contains(&livelli) || cerca > CERCA_MAX {
        return Err(Errore::Formato("livelli o raggio di ricerca"));
    }
    let piatti = strati::decodifica_valori(&st.griglia, &p.vettori, 2, -2 * cerca as i32, 2 * cerca as i32);
    Ok((passo, croma, livelli, piatti.chunks_exact(2).map(|c| (c[0], c[1])).collect()))
}

/// Decodifica tutti i fotogrammi. Un previsto con il dettaglio a metà o vuoto esce intero, meno fine.
pub fn decodifica_reel(r: &Reel) -> Result<Vec<Immagine>, Errore> {
    let mut stato: Option<Stato> = None;
    let mut out = Vec::with_capacity(r.fotogrammi.len());
    for f in &r.fotogrammi {
        match f {
            Fotogramma::Chiave(c) => {
                let (rif, g) = decodifica_piani(c, Strati::Tutti)?;
                out.push(piani_in_rgb(&rif));
                stato = Some(Stato::nuovo(g, rif));
            }
            Fotogramma::Previsto(p) => {
                let st = stato.as_mut().ok_or(Errore::Formato("fotogramma previsto senza una chiave prima"))?;
                let (passo, croma, livelli, v) = leggi_vettori(st, p)?;
                let pred = prevedi(&st.rif, &st.mappa, &v);
                let fine = if passo > 0 && !p.dettaglio.is_empty() { dettaglio::decodifica(&p.dettaglio, &pred, passo, croma, livelli, &st.mappa, st.griglia.celle(), Base::Tutto) } else { pred };
                out.push(piani_in_rgb(&fine));
                st.rif = fine;
                st.vettori = v;
            }
        }
    }
    Ok(out)
}

impl Reel {
    pub fn byte_totali(&self) -> usize {
        self.fotogrammi.iter().map(|f| f.byte_totali()).sum()
    }

    /// Gli spostamenti per cella del fotogramma `i`, in mezzi pixel (vuoto per una chiave): per misurare e guardare.
    pub fn vettori(&self, i: usize) -> Result<Vec<(i32, i32)>, Errore> {
        let f = self.fotogrammi.get(i).ok_or(Errore::Formato("fotogramma inesistente"))?;
        let Fotogramma::Previsto(p) = f else { return Ok(vec![]) };
        let chiave = self.fotogrammi[..i].iter().rev().find_map(|f| if let Fotogramma::Chiave(c) = f { Some(c) } else { None }).ok_or(Errore::Formato("fotogramma previsto senza una chiave prima"))?;
        let t = crate::leggi_testa(&chiave.testa)?;
        let st = Stato { mappa: vec![], pixel: vec![], rif: Piani { y: Piano::nuovo(0, 0), cb: Piano::nuovo(0, 0), cr: Piano::nuovo(0, 0) }, vettori: vec![], griglia: t.griglia };
        Ok(leggi_vettori(&st, p)?.3)
    }

    /// Tutto il reel in un elenco di byte: magia, versione, numero di fotogrammi, poi per ognuno il tipo e i suoi
    /// pezzi con la lunghezza davanti.
    pub fn in_byte(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(self.byte_totali() + 16 * self.fotogrammi.len());
        b.extend_from_slice(MAGIA_REEL);
        b.push(VERSIONE_REEL);
        scrivi_varint(&mut b, self.fotogrammi.len() as u64);
        let pezzo = |b: &mut Vec<u8>, p: &[u8]| {
            scrivi_varint(b, p.len() as u64);
            b.extend_from_slice(p);
        };
        for f in &self.fotogrammi {
            match f {
                Fotogramma::Chiave(c) => {
                    b.push(0);
                    for p in [&c.testa, &c.colore, &c.pendenza, &c.dettaglio] {
                        pezzo(&mut b, p);
                    }
                }
                Fotogramma::Previsto(p) => {
                    b.push(1);
                    for q in [&p.testa, &p.vettori, &p.dettaglio] {
                        pezzo(&mut b, q);
                    }
                }
            }
        }
        b
    }

    pub fn da_byte(b: &[u8]) -> Result<Reel, Errore> {
        if b.len() < 6 || &b[0..4] != MAGIA_REEL {
            return Err(Errore::Formato("intestazione del reel"));
        }
        if b[4] != VERSIONE_REEL {
            return Err(Errore::Formato("versione del reel sconosciuta"));
        }
        let mut i = 5;
        let n = leggi_varint(b, &mut i)? as usize;
        if n > 1 << 20 {
            return Err(Errore::Formato("troppi fotogrammi"));
        }
        let pezzo = |i: &mut usize| -> Result<Vec<u8>, Errore> {
            let l = leggi_varint(b, i)? as usize;
            let fine = i.checked_add(l).filter(|&f| f <= b.len()).ok_or(Errore::Formato("pezzo troncato"))?;
            let p = b[*i..fine].to_vec();
            *i = fine;
            Ok(p)
        };
        let mut fotogrammi = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            let tipo = *b.get(i).ok_or(Errore::Formato("fotogramma troncato"))?;
            i += 1;
            match tipo {
                0 => {
                    let (testa, colore, pendenza, dettaglio) = (pezzo(&mut i)?, pezzo(&mut i)?, pezzo(&mut i)?, pezzo(&mut i)?);
                    fotogrammi.push(Fotogramma::Chiave(Codificata { testa, colore, pendenza, dettaglio }));
                }
                1 => {
                    let (testa, vettori, dettaglio) = (pezzo(&mut i)?, pezzo(&mut i)?, pezzo(&mut i)?);
                    fotogrammi.push(Fotogramma::Previsto(Previsto { testa, vettori, dettaglio }));
                }
                _ => return Err(Errore::Formato("tipo di fotogramma sconosciuto")),
            }
        }
        Ok(Reel { fotogrammi })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::psnr_x100;

    /// Una scena con texture (scacchi da 16 pixel con una trama fine sopra, colori che cambiano piano) vista da
    /// `(sx, sy)`: spostare la finestra è come muovere la camera.
    fn scena(w: u32, h: u32, sx: i32, sy: i32, fase: i32) -> Immagine {
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let (xx, yy) = (x + sx + 1000, y + sy + 1000);
                let scacco = ((xx / 16 + yy / 16 + fase) % 2) * 60;
                let trama = (xx * 7 + yy * 3) % 23;
                let l = (90 + scacco + trama).clamp(0, 255);
                let r = (l + (xx % 64) / 2 - 16).clamp(0, 255) as u8;
                let g = l as u8;
                let b = (l - (yy % 64) / 2 + 16).clamp(0, 255) as u8;
                rgb.extend_from_slice(&[r, g, b]);
            }
        }
        Immagine { larghezza: w, altezza: h, rgb }
    }

    /// Tutt'altra scena: quasi buia, con un gradiente leggero. (Un gradiente pieno da 0 a 255 sta a 45 livelli di
    /// scarto medio dalla scacchiera da fermo, ma la ricerca degli spostamenti lo porta a 24: cella per cella trova
    /// sempre qualcosa che somiglia. La soglia del cambio di scena va letta così.)
    fn gradiente(w: u32, h: u32) -> Immagine {
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                let l = (x * 40 / w.max(1)) as u8;
                rgb.extend_from_slice(&[l, (y * 40 / h.max(1)) as u8, 40 - l]);
            }
        }
        Immagine { larghezza: w, altezza: h, rgb }
    }

    fn psnr_minimo(orig: &[Immagine], dec: &[Immagine]) -> u32 {
        orig.iter().zip(dec).map(|(a, b)| psnr_x100(a, b).unwrap()).min().unwrap()
    }

    fn par() -> ParametriReel {
        ParametriReel { foto: Parametri { raggio: 12, ..Parametri::default() }, chiave_ogni: 0, cerca: 8, soglia_chiave: 40 }
    }

    #[test]
    fn reel_fermo_costa_quasi_niente() {
        let f: Vec<Immagine> = (0..5).map(|_| scena(192, 128, 0, 0, 0)).collect();
        let r = codifica_reel(&f, par()).unwrap();
        assert_eq!(r.fotogrammi.len(), 5);
        let chiave = r.fotogrammi[0].byte_totali();
        assert!(matches!(r.fotogrammi[0], Fotogramma::Chiave(_)));
        for p in &r.fotogrammi[1..] {
            assert!(matches!(p, Fotogramma::Previsto(_)));
            assert!(p.byte_totali() * 20 < chiave, "fermo: previsto {} byte contro chiave {}", p.byte_totali(), chiave);
        }
        let d = decodifica_reel(&r).unwrap();
        assert_eq!(d.len(), 5);
        assert!(psnr_minimo(&f, &d) >= 3000, "PSNR {}", psnr_minimo(&f, &d));
        // quasi tutte ferme: qualche cella puo' preferire il mezzo pixel, che liscia l'errore di quantizzazione
        let v = r.vettori(1).unwrap();
        let ferme = v.iter().filter(|&&s| s == (0, 0)).count();
        assert!(ferme * 10 >= v.len() * 9, "celle ferme {ferme} su {}", v.len());
    }

    #[test]
    fn reel_che_scorre_trova_lo_spostamento() {
        // la camera va di (3, −2) a ogni fotogramma: il pixel x di oggi era in x + 3 ieri, cioè (6, −4) mezzi pixel
        let f: Vec<Immagine> = (0..6).map(|i| scena(192, 128, 3 * i, -2 * i, 0)).collect();
        let r = codifica_reel(&f, par()).unwrap();
        let chiave = r.fotogrammi[0].byte_totali();
        let v = r.vettori(1).unwrap();
        let giusti = v.iter().filter(|&&s| s == (6, -4)).count();
        assert!(giusti * 10 >= v.len() * 8, "spostamento (6,−4) trovato in {giusti} celle su {}", v.len());
        for p in &r.fotogrammi[1..] {
            assert!(p.byte_totali() * 3 < chiave, "scorre: previsto {} byte contro chiave {}", p.byte_totali(), chiave);
        }
        let d = decodifica_reel(&r).unwrap();
        assert!(psnr_minimo(&f, &d) >= 2800, "PSNR {}", psnr_minimo(&f, &d));
        // senza cercare lo spostamento costa di più
        let fermo = codifica_reel(&f, ParametriReel { cerca: 0, ..par() }).unwrap();
        assert!(fermo.byte_totali() > r.byte_totali(), "senza spostamento {} byte, con {}", fermo.byte_totali(), r.byte_totali());
    }

    #[test]
    fn cambio_di_scena_fa_una_chiave() {
        let f = vec![scena(160, 96, 0, 0, 0), scena(160, 96, 0, 0, 0), gradiente(160, 96), gradiente(160, 96)];

        let r = codifica_reel(&f, ParametriReel { soglia_chiave: 24, ..par() }).unwrap();
        let tipi: Vec<bool> = r.fotogrammi.iter().map(|f| matches!(f, Fotogramma::Chiave(_))).collect();
        assert_eq!(tipi, vec![true, false, true, false]);
        // chiave forzata ogni 2
        let r2 = codifica_reel(&f, ParametriReel { chiave_ogni: 2, soglia_chiave: 0, ..par() }).unwrap();
        let tipi2: Vec<bool> = r2.fotogrammi.iter().map(|f| matches!(f, Fotogramma::Chiave(_))).collect();
        assert_eq!(tipi2, vec![true, false, true, false]);
    }

    #[test]
    fn in_byte_e_ritorno_troncature_e_spazzatura() {
        let f: Vec<Immagine> = (0..4).map(|i| scena(128, 96, 2 * i, i, 0)).collect();
        let r = codifica_reel(&f, par()).unwrap();
        let b = r.in_byte();
        assert_eq!(Reel::da_byte(&b).unwrap(), r);
        assert!(Reel::da_byte(&b[..b.len() - 10]).is_err());
        assert!(Reel::da_byte(&[]).is_err());
        // un previsto con il dettaglio a metà o vuoto: immagine intera, nessun panico
        let mut tronco = r.clone();
        if let Fotogramma::Previsto(p) = &mut tronco.fotogrammi[2] {
            p.dettaglio.truncate(p.dettaglio.len() / 2);
        }
        if let Fotogramma::Previsto(p) = &mut tronco.fotogrammi[3] {
            p.dettaglio.clear();
        }
        let d = decodifica_reel(&tronco).unwrap();
        assert_eq!(d.len(), 4);
        assert!(psnr_x100(&f[3], &d[3]).unwrap() >= 2000);
        // byte a caso dentro i vettori e il dettaglio: errore o immagine, mai panico
        let mut rotto = r.clone();
        if let Fotogramma::Previsto(p) = &mut rotto.fotogrammi[1] {
            for (i, x) in p.vettori.iter_mut().enumerate() {
                *x ^= (i as u8).wrapping_mul(97);
            }
            for (i, x) in p.dettaglio.iter_mut().enumerate() {
                *x = x.wrapping_add(i as u8);
            }
        }
        let _ = decodifica_reel(&rotto);
        // un previsto senza chiave prima è un errore
        let orfano = Reel { fotogrammi: vec![r.fotogrammi[1].clone()] };
        assert!(decodifica_reel(&orfano).is_err());
    }
}
