//! Reel: fotogrammi uno dopo l'altro sulla stessa griglia ad alveare.
//!
//! Il primo fotogramma (e uno ogni tanto, o a un cambio di scena) è una **chiave**: una foto a celle come in
//! `lib.rs`, con i suoi tre strati. Gli altri sono **previsti**: per ogni cella si cerca di quanto si è spostata
//! rispetto a un fotogramma già ricostruito (uno spostamento `(dx, dy)` per cella in mezzi pixel, solo somme di
//! differenze assolute e medie), il riferimento spostato cella per cella fa da base, e il resto passa per la stessa
//! trasformata a onde e la stessa codifica aritmetica del dettaglio delle foto (`dettaglio`, con `Base::Tutto`: qui
//! la base somiglia davvero all'immagine, bande alte comprese). Gli spostamenti viaggiano compressi come gli strati
//! 0 e 1 (`strati`: previsti dalle celle vicine). La griglia è quella della chiave: non si trasmette di nuovo.
//!
//! **Bidirezionali** (dal 13/11/2026, come i B‑frame a piramide di x264): tra un'àncora e l'altra (chiave o
//! previsto) i fotogrammi in mezzo si prevedono da **due** riferimenti, uno prima e uno dopo; ogni cella sceglie
//! da dove: dal passato, dal futuro o dalla media dei due (ferma il rumore, copre le zone che si scoprono). Il
//! fotogramma di mezzo si codifica per primo e fa da riferimento ai due quarti. L'ordine dei byte è quello di
//! decodifica; ogni fotogramma dice a che punto si mostra e quali riferimenti usa. **Il guadagno vero sta nel passo
//! più grosso in fondo alla piramide** (`passo_a`): la doppia previsione da sola, su video rumorosi, rende il 2 %;
//! con le foglie quantizzate più grosse si risparmia un quarto dei byte a pari PSNR (misure nel README).
//!
//! **Celle che seguono il moto** (13/11/2026, idea del fondatore: "celle intelligenti che predicono la dimensione in
//! base a quando il moto cambia"): la griglia di un previsto non è più quella della chiave. Ogni cella base può
//! essere intera o divisa in sette, e chi legge **prevede da solo** quali dividere dagli spostamenti del fotogramma
//! prima (`raffinate_previste`): finché dentro una cella divisa i sette spostamenti non sono uguali la cella resta
//! piccola; quando il moto torna uniforme torna grande da sola. Zero byte. Le divisioni nuove le decide chi scrive
//! con un bit adattivo per cella base (`divisioni`, nessun byte quando non cambia niente): divide dove lo scarto
//! è alto e la divisione paga, riunisce dove dividere non paga più.
//!
//! L'alveare al lavoro sul video: dentro ogni fotogramma previsto il resto arriva a pezzi, cella per cella, con le
//! celle in ordine di quanto la previsione ha sbagliato (`dettaglio::percorri`): prima quelle che si muovono o si
//! scoprono, per ultime quelle ferme. Un fotogramma previsto troncato è intero (la base c'è sempre), solo meno fine
//! dove la banda non è arrivata.
//!
//! Chi codifica tiene come riferimento la **propria ricostruzione** (quella che vede chi legge con tutti i pezzi):
//! chi legge un reel a pezzi troncati si allontana un po' a ogni fotogramma, fino alla chiave dopo.

use crate::aritmetica::{Lettore, Prob, Scrittore};
use crate::dettaglio::{self, Base, Contesti, Piani, Piano, LIVELLI_MAX};
use crate::{codifica, contesti_nuovi, decodifica_piani, leggi_varint, piani_di, piani_in_rgb, scrivi_varint, strati, ycbcr, Codificata, Errore, Griglia, Immagine, Parametri, Strati, S};

/// Versione del formato del reel.
pub const VERSIONE_REEL: u8 = 4;
const MAGIA_REEL: &[u8; 4] = b"M4RL";
/// Spostamento massimo per cella che il formato sa scrivere (pixel).
pub const CERCA_MAX: u8 = 32;
/// Quanti fotogrammi ricostruiti si tengono come riferimento (bastano per una piramide tra àncore a distanza 8).
const RIFERIMENTI: usize = 12;
/// Di quanti ottavi cresce il passo a ogni livello della piramide dei bidirezionali (vedi `passo_a`).
const GROSSEZZA_B: u8 = 3;
/// Due spostamenti distano "abbastanza" (mezzi pixel, |dx|+|dy|) perché la cella tra loro vada divisa.
const SOGLIA_MOTO: i32 = 4;
/// Una cella intera con scarto medio per pixel sopra `SOGLIA_SCARTO_OTTAVI/8 × passo` si prova a dividere (basso
/// apposta: con celle grandi un soggetto piccolo che si muove alza poco la media; misurato il 13/11/2026, soglie
/// 8/8 → 1/8: su un video con la camera ferma le celle non si dividevano mai)…
const SOGLIA_SCARTO_OTTAVI: i64 = 1;
/// …e si divide se lo scarto cala di più di `PENALE_DIVISIONE × passo` (sei spostamenti in più costano byte;
/// misurato 24 → 12 → 6: 12 è il meglio su due video).
const PENALE_DIVISIONE: i64 = 12;
/// Sotto questi pixel una cella non si divide (ritagli ai bordi dell'immagine).
const PIXEL_MIN_DIVISIONE: usize = 64;

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
    /// Quanti fotogrammi bidirezionali tra un'àncora e l'altra (0 = nessuno, 3 = piramide di quattro come x264).
    pub bidirezionali: u8,
    /// Le celle seguono il moto (si dividono dove non è uniforme). Solo chi scrive decide: chi legge segue i bit.
    pub celle_mobili: bool,
    /// Fotogrammi al secondo di chi guarda (1–255): scritto nel reel, non cambia la codifica.
    pub fps: u8,
}

impl Default for ParametriReel {
    fn default() -> Self {
        Self { foto: Parametri::default(), chiave_ogni: 60, cerca: 16, soglia_chiave: 24, bidirezionali: 3, celle_mobili: true, fps: 24 }
    }
}

/// Un fotogramma previsto da uno o due riferimenti: pezzi separati, ognuno può essere un blob.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Previsto {
    /// passo, passo dei colori, livelli, raggio di ricerca, poi (varint) quando si mostra e i riferimenti, tutti
    /// come indici dalla chiave: `mostra`, `rif_a` e, se bidirezionale, `rif_b`
    pub testa: Vec<u8>,
    /// per cella base un bit: la divisione in sette è diversa da quella prevista; vuoto = tutto come previsto
    pub divisioni: Vec<u8>,
    /// per cella da dove si prevede (0 passato, 1 futuro, 2 media), compresso; vuoto se non bidirezionale
    pub modi: Vec<u8>,
    /// spostamento (dx, dy) per cella verso il primo riferimento, compresso
    pub vettori: Vec<u8>,
    /// spostamento per cella verso il secondo riferimento, compresso; vuoto se non bidirezionale
    pub vettori_b: Vec<u8>,
    /// il resto, a pezzi cella per cella
    pub dettaglio: Vec<u8>,
}

impl Previsto {
    pub fn byte_totali(&self) -> usize {
        self.testa.len() + self.divisioni.len() + self.modi.len() + self.vettori.len() + self.vettori_b.len() + self.dettaglio.len()
    }
    pub fn bidirezionale(&self) -> bool {
        !self.vettori_b.is_empty()
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

/// I fotogrammi in ordine di **decodifica** (un bidirezionale viene dopo le sue due àncore).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Reel {
    pub fotogrammi: Vec<Fotogramma>,
    /// fotogrammi al secondo (dalla VERSIONE 4; 0 nei reel vecchi non esiste: si rifiuta)
    pub fps: u8,
}

/// La geometria di una griglia: quello che serve per prevedere e codificare un fotogramma su di essa.
#[derive(Clone)]
struct Geometria {
    griglia: Griglia,
    mappa: Vec<u32>,
    /// gli indici dei pixel di ogni cella
    pixel: Vec<Vec<u32>>,
    /// per ogni pixel la cella oltre il bordo più vicino e il suo peso in ottavi (0–4): la previsione si sfuma
    vicina: Vec<u32>,
    peso: Vec<u8>,
}

impl Geometria {
    fn nuova(griglia: Griglia) -> Self {
        let (mappa, vicina, peso) = griglia.mappa_e_vicine();
        let mut pixel = vec![Vec::new(); griglia.celle()];
        for (k, &c) in mappa.iter().enumerate() {
            pixel[c as usize].push(k as u32);
        }
        Geometria { griglia, mappa, pixel, vicina, peso }
    }
}

/// Quello che resta da un fotogramma all'altro, uguale da una parte e dall'altra.
struct Stato {
    /// la griglia della chiave: i previsti tengono il suo reticolo e il suo raggio, cambiano solo le divisioni
    base: Griglia,
    /// la geometria dell'ultimo fotogramma codificato
    geo: Geometria,
    /// i fotogrammi ricostruiti del gruppo (dalla chiave), per indice di visualizzazione
    rif: Vec<(u32, Piani)>,
    /// gli spostamenti dell'ultimo fotogramma codificato, sulle celle di `geo` (candidati e regola per il prossimo)
    vettori: Vec<(i32, i32)>,
    /// le probabilità della codifica aritmetica del resto, tenute da un fotogramma all'altro (da zero a ogni chiave)
    ctx: [Contesti; 2],
}

impl Stato {
    fn nuovo(griglia: Griglia, chiave: Piani) -> Self {
        let geo = Geometria::nuova(griglia.clone());
        let n = geo.griglia.celle();
        Stato { base: griglia, geo, rif: vec![(0, chiave)], vettori: vec![(0, 0); n], ctx: contesti_nuovi() }
    }
    /// La geometria per queste divisioni: quella dell'ultimo fotogramma se è uguale (il caso normale: rifarla costa
    /// ~50 ms a fotogramma), altrimenti nuova.
    fn geometria_per(&self, raffinate: &[bool]) -> Geometria {
        if self.geo.griglia.raffinate() == raffinate {
            self.geo.clone()
        } else {
            Geometria::nuova(self.base.clone().con_raffinate(raffinate.to_vec()))
        }
    }
    fn riferimento(&self, mostra: u32) -> Option<&Piani> {
        self.rif.iter().find(|(m, _)| *m == mostra).map(|(_, p)| p)
    }
    fn tieni(&mut self, mostra: u32, p: Piani) {
        self.rif.retain(|(m, _)| *m != mostra);
        self.rif.push((mostra, p));
        if self.rif.len() > RIFERIMENTI {
            self.rif.remove(0);
        }
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
/// (costano byte e fanno bordi). `cand` è lo spostamento della stessa zona nel fotogramma prima, per cella.
/// Restituisce gli spostamenti e lo scarto della luce per cella.
fn cerca_vettori(cur: &Piano, rif: &Piano, geo: &Geometria, cand: &[(i32, i32)], cerca: i32, passo: u8) -> (Vec<(i32, i32)>, Vec<i64>) {
    let n = geo.griglia.celle();
    let mut v = vec![(0i32, 0i32); n];
    let mut piatti = vec![0i32; n * 2];
    let mut scarti = vec![0i64; n];
    let lambda = passo.max(1) as i64;
    let lim = 2 * cerca;
    for (c0, sinistra, sopra, quante) in strati::ordine(&geo.griglia) {
        for c in c0..c0 + quante {
            let pix = &geo.pixel[c];
            let (px, py) = if c == c0 { (strati::previsione(&piatti, 2, 0, sinistra, sopra), strati::previsione(&piatti, 2, 1, sinistra, sopra)) } else { v[c0] };
            if pix.is_empty() {
                // fuori dall'immagine: lo spostamento previsto, che non costa niente e non sembra un moto diverso
                v[c] = (px.clamp(-lim, lim), py.clamp(-lim, lim));
                piatti[c * 2] = v[c].0;
                piatti[c * 2 + 1] = v[c].1;
                continue;
            }
            if cerca == 0 {
                scarti[c] = scarto(cur, rif, pix, 0, 0, 1);
                continue;
            }
            let costo = |hx: i32, hy: i32| scarto(cur, rif, pix, hx, hy, 1) + lambda * ((hx - px).abs() + (hy - py).abs()) as i64;
            let mut candidati = vec![(0, 0), (px, py), cand[c]];
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
            scarti[c] = scarto(cur, rif, pix, migliore.1 .0, migliore.1 .1, 1);
        }
    }
    (v, scarti)
}

/// Le celle che non usano un riferimento prendono lo spostamento previsto dalle vicine: costa quasi niente.
fn azzera_inutili(g: &Griglia, v: &mut [(i32, i32)], usa: &[bool]) {
    let n = v.len();
    let mut piatti = vec![0i32; n * 2];
    for (c0, sinistra, sopra, quante) in strati::ordine(g) {
        for c in c0..c0 + quante {
            if !usa[c] {
                v[c] = if c == c0 { (strati::previsione(&piatti, 2, 0, sinistra, sopra), strati::previsione(&piatti, 2, 1, sinistra, sopra)) } else { v[c0] };
            }
            piatti[c * 2] = v[c].0;
            piatti[c * 2 + 1] = v[c].1;
        }
    }
}

// ─── Celle che seguono il moto ──────────────────────────────────────────────

/// Quali celle base restano divise nel prossimo fotogramma, viste dagli spostamenti dell'ultimo (`g`, `v`): una
/// cella divisa resta divisa finché dentro il moto non è uniforme (uno dei sette spostamenti dista più di
/// `SOGLIA_MOTO` dal centro), poi torna intera da sola. Le divisioni nuove le dice chi scrive con i bit: dagli
/// spostamenti non si vede se un confine passa **dentro** una cella intera o **tra** due (provato: prevedere la
/// divisione quando due vicine intere si muovono diversamente sbagliava quasi sempre, e ogni errore costa un bit).
/// Chi scrive e chi legge la calcolano uguale: zero byte.
fn raffinate_previste(g: &Griglia, v: &[(i32, i32)]) -> Vec<bool> {
    let dist = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs() + (a.1 - b.1).abs();
    (0..g.celle_base())
        .map(|k| {
            let c0 = g.inizio[k] as usize;
            g.raffinate[k] && (c0 + 1..c0 + 7).any(|c| dist(v[c], v[c0]) > SOGLIA_MOTO)
        })
        .collect()
}

/// Il campo di moto da tenere per il fotogramma dopo (regola delle divisioni e candidati): gli spostamenti verso il
/// primo riferimento, ma per le celle che guardano solo al futuro (modo 1) lo spostamento verso il secondo
/// riferimento girato e scalato alla distanza del primo: è il moto vero della cella, non il valore di comodo che
/// `azzera_inutili` le dà per costare poco.
fn campo_di_moto(va: &[(i32, i32)], vb: &[(i32, i32)], modi: &[u8], t: &TestaPrevisto) -> Vec<(i32, i32)> {
    let Some(rif_b) = t.rif_b else { return va.to_vec() };
    let (da, db) = (t.mostra as i64 - t.rif_a as i64, rif_b as i64 - t.mostra as i64);
    if db <= 0 || da <= 0 || modi.len() != va.len() || vb.len() != va.len() {
        return va.to_vec();
    }
    va.iter().zip(vb).zip(modi).map(|((&a, &b), &m)| if m == 1 { ((-(b.0 as i64) * da / db) as i32, (-(b.1 as i64) * da / db) as i32) } else { a }).collect()
}

/// Per ogni cella della griglia nuova lo spostamento che aveva la stessa zona (il pixel al centro) nel fotogramma
/// prima: il candidato "stessa cella" della ricerca.
fn candidati(st: &Stato, g: &Griglia) -> Vec<(i32, i32)> {
    let (w, h) = (g.larghezza as i64 - 1, g.altezza as i64 - 1);
    (0..g.celle() as u32)
        .map(|c| {
            let (cx, cy) = g.centro(c);
            st.vettori[st.geo.griglia.cella_di((cx / S).clamp(0, w) as u32, (cy / S).clamp(0, h) as u32) as usize]
        })
        .collect()
}

/// Un bit per cella base: la divisione reale è diversa da quella prevista. Contesto: la previsione e il bit prima.
/// Tutto come previsto → nessun byte.
fn codifica_divisioni(previste: &[bool], reali: &[bool]) -> Vec<u8> {
    if previste == reali {
        return vec![];
    }
    let mut sc = Scrittore::nuovo();
    let mut pr = [Prob::NUOVA; 4];
    let mut prima = false;
    for (&p, &r) in previste.iter().zip(reali) {
        let f = p != r;
        sc.bit(&mut pr[p as usize * 2 + prima as usize], f);
        prima = f;
    }
    sc.chiudi()
}

fn decodifica_divisioni(b: &[u8], previste: &[bool]) -> Vec<bool> {
    if b.is_empty() {
        return previste.to_vec();
    }
    let mut le = Lettore::nuovo(b);
    let mut pr = [Prob::NUOVA; 4];
    let mut prima = false;
    previste
        .iter()
        .map(|&p| {
            let f = le.bit(&mut pr[p as usize * 2 + prima as usize]);
            prima = f;
            p != f
        })
        .collect()
}

/// La griglia di un previsto e gli spostamenti verso il primo riferimento, scelti da chi scrive.
struct Scelta {
    geo: Geometria,
    va: Vec<(i32, i32)>,
    scarti: Vec<i64>,
    divisioni: Vec<u8>,
}

/// Chi scrive sceglie la griglia: parte da quella prevista (come chi legge), cerca gli spostamenti; riunisce le
/// celle divise con sette spostamenti uguali; prova a dividere le celle intere con scarto alto e tiene la divisione
/// solo se lo scarto cala più della penale. Al massimo tre ricerche.
fn scegli_griglia(st: &Stato, cur: &Piano, rif: &Piano, cerca: i32, passo: u8, celle_mobili: bool) -> Scelta {
    let previste = raffinate_previste(&st.geo.griglia, &st.vettori);
    let cerca_su = |raff: &[bool]| {
        let geo = st.geometria_per(raff);
        let cand = candidati(st, &geo.griglia);
        let (v, sc) = cerca_vettori(cur, rif, &geo, &cand, cerca, passo);
        (geo, v, sc)
    };
    let (geo0, v0, s0) = cerca_su(&previste);
    if !celle_mobili {
        return Scelta { geo: geo0, va: v0, scarti: s0, divisioni: vec![] };
    }
    let p = passo.max(1) as i64;
    let g0 = &geo0.griglia;
    let mut prova = previste.clone();
    let mut da_decidere = Vec::new();
    for k in 0..g0.celle_base() {
        let c0 = g0.inizio[k] as usize;
        if g0.raffinate[k] {
            // divisa: quanto costerebbe intera, con lo spostamento del centro su tutte e sette? se la differenza
            // non supera la penale, si riunisce
            let (hx, hy) = v0[c0];
            let unita: i64 = (c0..c0 + 7).map(|c| scarto(cur, rif, &geo0.pixel[c], hx, hy, 1)).sum();
            let divisa: i64 = (c0..c0 + 7).map(|c| s0[c]).sum();
            if unita - divisa <= PENALE_DIVISIONE * p {
                prova[k] = false;
            }
        } else if geo0.pixel[c0].len() >= PIXEL_MIN_DIVISIONE && s0[c0] * 8 > SOGLIA_SCARTO_OTTAVI * p * geo0.pixel[c0].len() as i64 {
            prova[k] = true;
            da_decidere.push(k);
        }
    }
    if prova == previste {
        return Scelta { geo: geo0, va: v0, scarti: s0, divisioni: vec![] };
    }
    let (geo1, v1, s1) = cerca_su(&prova);
    let mut finale = prova.clone();
    for &k in &da_decidere {
        let c1 = geo1.griglia.inizio[k] as usize;
        let dopo: i64 = (c1..c1 + 7).map(|c| s1[c]).sum();
        if s0[g0.inizio[k] as usize] - dopo <= PENALE_DIVISIONE * p {
            finale[k] = false;
        }
    }
    let (geo, va, scarti) = if finale == prova { (geo1, v1, s1) } else { cerca_su(&finale) };
    let divisioni = codifica_divisioni(&previste, &finale);
    Scelta { geo, va, scarti, divisioni }
}

/// I due riferimenti di un fotogramma e gli spostamenti verso ognuno; `modi` dice per cella quale si usa
/// (0 il primo, 1 il secondo, 2 la media). Per un previsto semplice `b` è `None` e `modi` è vuoto.
struct Previsione<'a> {
    a: &'a Piani,
    b: Option<&'a Piani>,
    va: &'a [(i32, i32)],
    vb: &'a [(i32, i32)],
    modi: &'a [u8],
}

impl Previsione<'_> {
    #[inline]
    fn modo(&self, c: usize) -> u8 {
        if self.b.is_none() {
            0
        } else {
            self.modi[c]
        }
    }
    /// La luce prevista dalla cella `c` nel pixel `(x, y)`.
    #[inline]
    fn luce(&self, c: usize, x: i32, y: i32) -> i32 {
        let (ax, ay) = self.va[c];
        match self.modo(c) {
            0 => campione(&self.a.y, x, y, ax, ay),
            1 => {
                let (bx, by) = self.vb[c];
                campione(&self.b.expect("secondo riferimento").y, x, y, bx, by)
            }
            _ => {
                let (bx, by) = self.vb[c];
                (campione(&self.a.y, x, y, ax, ay) + campione(&self.b.expect("secondo riferimento").y, x, y, bx, by) + 1) >> 1
            }
        }
    }
    /// I due colori previsti dalla cella `c` nel pixel dimezzato `(x, y)`.
    #[inline]
    fn colori(&self, c: usize, x: i32, y: i32) -> (i32, i32) {
        let (ax, ay) = self.va[c];
        match self.modo(c) {
            0 => (campione_quarto(&self.a.cb, x, y, ax, ay), campione_quarto(&self.a.cr, x, y, ax, ay)),
            1 => {
                let (bx, by) = self.vb[c];
                let b = self.b.expect("secondo riferimento");
                (campione_quarto(&b.cb, x, y, bx, by), campione_quarto(&b.cr, x, y, bx, by))
            }
            _ => {
                let (bx, by) = self.vb[c];
                let b = self.b.expect("secondo riferimento");
                ((campione_quarto(&self.a.cb, x, y, ax, ay) + campione_quarto(&b.cb, x, y, bx, by) + 1) >> 1, (campione_quarto(&self.a.cr, x, y, ax, ay) + campione_quarto(&b.cr, x, y, bx, by) + 1) >> 1)
            }
        }
    }
    /// Due celle prevedono allo stesso modo?
    #[inline]
    fn uguali(&self, c: usize, d: usize) -> bool {
        let m = self.modo(c);
        m == self.modo(d) && (m == 1 || self.va[c] == self.va[d]) && (m == 0 || self.vb[c] == self.vb[d])
    }
}

/// Il riferimento (o i due) spostato cella per cella (mezzi pixel sulla luce, quarti sui colori dimezzati): la base
/// del fotogramma previsto. Vicino al bordo tra due celle che prevedono in modo diverso la previsione è una media
/// pesata delle due (come l'OBMC di H.263): niente gradini artificiali, che costerebbero byte nel resto.
fn prevedi(st: &Geometria, pr: &Previsione) -> Piani {
    let (w, h) = (pr.a.y.w, pr.a.y.h);
    let mut y = Piano::nuovo(w, h);
    for k in 0..w * h {
        let (c, d) = (st.mappa[k] as usize, st.vicina[k] as usize);
        let (x, yy) = ((k % w) as i32, (k / w) as i32);
        let a = pr.luce(c, x, yy);
        let p = st.peso[k] as i32;
        y.v[k] = if p > 0 && !pr.uguali(c, d) { ((8 - p) * a + p * pr.luce(d, x, yy) + 4) >> 3 } else { a };
    }
    let (wc, hc) = (pr.a.cb.w, pr.a.cb.h);
    let mut cb = Piano::nuovo(wc, hc);
    let mut cr = Piano::nuovo(wc, hc);
    for yc in 0..hc {
        for xc in 0..wc {
            let k = (2 * yc).min(h - 1) * w + (2 * xc).min(w - 1);
            let (c, d) = (st.mappa[k] as usize, st.vicina[k] as usize);
            let p = st.peso[k] as i32;
            let (a, b) = pr.colori(c, xc as i32, yc as i32);
            let (a, b) = if p > 0 && !pr.uguali(c, d) {
                let (a2, b2) = pr.colori(d, xc as i32, yc as i32);
                (((8 - p) * a + p * a2 + 4) >> 3, ((8 - p) * b + p * b2 + 4) >> 3)
            } else {
                (a, b)
            };
            cb.v[yc * wc + xc] = a;
            cr.v[yc * wc + xc] = b;
        }
    }
    Piani { y, cb, cr }
}

// ─── Modi per cella (bidirezionali) ─────────────────────────────────────────

/// Per cella da dove si prevede: due bit adattivi con il contesto del modo della cella prima.
fn codifica_modi(modi: &[u8]) -> Vec<u8> {
    let mut sc = Scrittore::nuovo();
    let mut pr = [Prob::NUOVA; 6];
    let mut prima = 0usize;
    for &m in modi {
        sc.bit(&mut pr[prima * 2], m == 2);
        if m != 2 {
            sc.bit(&mut pr[prima * 2 + 1], m == 1);
        }
        prima = m as usize;
    }
    sc.chiudi()
}

fn decodifica_modi(b: &[u8], celle: usize) -> Vec<u8> {
    let mut le = Lettore::nuovo(b);
    let mut pr = [Prob::NUOVA; 6];
    let mut prima = 0usize;
    let mut out = Vec::with_capacity(celle);
    for _ in 0..celle {
        let m = if le.bit(&mut pr[prima * 2]) { 2 } else { le.bit(&mut pr[prima * 2 + 1]) as u8 };
        out.push(m);
        prima = m as usize;
    }
    out
}

// ─── Codifica e decodifica ──────────────────────────────────────────────────

/// Un fotogramma da codificare: dove si mostra (dalla chiave) e i riferimenti.
struct Compito {
    indice: usize,
    mostra: u32,
    rif_a: u32,
    rif_b: Option<u32>,
    /// 0 per un'àncora, 1 per il fotogramma di mezzo, 2 per i quarti…: più in fondo, più grosso il passo
    profondita: u8,
}

/// Il passo di un fotogramma a una certa profondità della piramide: +3/8 per livello (×1,375 il mezzo, ×1,75 i
/// quarti). I fotogrammi in fondo non fanno da riferimento a nessuno (o a pochi): si possono quantizzare più grossi
/// senza che l'errore si propaghi, e tra due riferimenti buoni l'occhio non se ne accorge (x264 fa lo stesso con i
/// B, `pbratio`). Misurato il 13/11/2026 su due video: è questo, non la doppia previsione in sé, che fa guadagnare
/// (a pari PSNR −21/−30 % con grossezza 2/8–4/8; 3/8 è il mezzo scelto).
fn passo_a(passo: u8, profondita: u8) -> u8 {
    (passo as u32 + passo as u32 * profondita as u32 * GROSSEZZA_B as u32 / 8).min(255) as u8
}

/// L'ordine di codifica tra due àncore `l` e `r` (già codificate): il mezzo, poi i due lati (piramide).
fn piramide(base: usize, l: u32, r: u32, profondita: u8, out: &mut Vec<Compito>) {
    if r <= l + 1 {
        return;
    }
    let m = (l + r) / 2;
    out.push(Compito { indice: base + m as usize, mostra: m, rif_a: l, rif_b: Some(r), profondita });
    piramide(base, l, m, profondita + 1, out);
    piramide(base, m, r, profondita + 1, out);
}

/// Codifica una sequenza di fotogrammi della stessa misura.
pub fn codifica_reel(fotogrammi: &[Immagine], p: ParametriReel) -> Result<Reel, Errore> {
    codifica_reel_con(fotogrammi, p, |_| {})
}

/// Come `codifica_reel`, con `avanza(fotogrammi codificati finora)` dopo ogni fotogramma (per chi mostra una
/// barra: un reel di un minuto costa minuti di calcolo).
pub fn codifica_reel_con(fotogrammi: &[Immagine], p: ParametriReel, mut avanza: impl FnMut(usize)) -> Result<Reel, Errore> {
    let primo = fotogrammi.first().ok_or(Errore::Immagine("nessun fotogramma"))?;
    let (w, h) = (primo.larghezza as usize, primo.altezza as usize);
    for img in fotogrammi {
        if img.larghezza != primo.larghezza || img.altezza != primo.altezza || img.rgb.len() != w * h * 3 {
            return Err(Errore::Immagine("fotogrammi di misure diverse"));
        }
    }
    let cerca = p.cerca.min(CERCA_MAX) as i32;
    let passo = p.foto.passo_dettaglio;
    let croma = p.foto.passo_croma;
    let livelli = p.foto.livelli.clamp(1, LIVELLI_MAX);
    let gruppo = p.bidirezionali as usize + 1;
    let n_tot = fotogrammi.len();
    let mut stato: Option<Stato> = None;
    let mut base = 0usize; // indice assoluto della chiave corrente
    let mut ancora = 0usize; // indice assoluto dell'ultima àncora codificata
    let mut out = Vec::with_capacity(n_tot);
    let piani = |i: usize| {
        let img = &fotogrammi[i];
        let luce: Vec<u8> = img.rgb.chunks_exact(3).map(|px| ycbcr(px[0], px[1], px[2]).0).collect();
        piani_di(img, &luce)
    };
    let mut i = 0usize;
    while i < n_tot {
        // questa è un'àncora: chiave programmata, o previsto dall'àncora prima (o chiave se la scena cambia)
        let orig = piani(i);
        let mut chiave = stato.is_none() || (p.chiave_ogni > 0 && i % p.chiave_ogni as usize == 0);
        let mut scelta: Option<(Scelta, u32, u32)> = None;
        if !chiave {
            let st = stato.as_mut().expect("stato");
            let (mostra, rif_a) = ((i - base) as u32, (ancora - base) as u32);
            let sc = scegli_griglia(st, &orig.y, &st.riferimento(rif_a).expect("riferimento").y, cerca, passo, p.celle_mobili);
            let tot: i64 = sc.scarti.iter().sum();
            if p.soglia_chiave != 0 && tot / (w * h) as i64 > p.soglia_chiave as i64 {
                chiave = true;
            } else {
                scelta = Some((sc, mostra, rif_a));
            }
        }
        if chiave {
            // i fotogrammi tra l'àncora prima e questa chiave (chiave programmata o cambio di scena) si codificano
            // PRIMA della chiave, in catena dall'àncora vecchia, uno dal precedente: così ogni gruppo che parte da
            // una chiave si legge da solo (un pezzo per la rete, `pacchetto`) e la piramide resta sul resto del reel.
            // Fino al 16/11 qui si ricodificava TUTTO il reel senza bidirezionali (anche per la chiave ogni 60).
            if let Some(st) = stato.as_mut() {
                for j in ancora + 1..i {
                    let orig_j = piani(j);
                    let (mostra, rif_a) = ((j - base) as u32, (j - 1 - base) as u32);
                    let sc = scegli_griglia(st, &orig_j.y, &st.riferimento(rif_a).expect("riferimento").y, cerca, passo, p.celle_mobili);
                    out.push(Fotogramma::Previsto(codifica_previsto(st, &orig_j, &Compito { indice: j, mostra, rif_a, rif_b: None, profondita: 0 }, sc.geo, sc.divisioni, sc.va, vec![], vec![], passo, croma, livelli, cerca)));
                }
            }
            let (cod, griglia) = codifica(&fotogrammi[i], p.foto)?;
            let (rif, _) = decodifica_piani(&cod, Strati::Tutti)?;
            stato = Some(Stato::nuovo(griglia, rif));
            base = i;
            out.push(Fotogramma::Chiave(cod));
        } else if let Some((sc, mostra, rif_a)) = scelta {
            let st = stato.as_mut().expect("stato");
            out.push(Fotogramma::Previsto(codifica_previsto(st, &orig, &Compito { indice: i, mostra, rif_a, rif_b: None, profondita: 0 }, sc.geo, sc.divisioni, sc.va, vec![], vec![], passo, croma, livelli, cerca)));
        }
        // i bidirezionali tra l'àncora prima e questa, in ordine di piramide (nessuno se questa è una chiave: i
        // fotogrammi in mezzo sono già usciti in catena)
        let st = stato.as_mut().expect("stato");
        let mut compiti = Vec::new();
        if !chiave {
            piramide(base, (ancora - base) as u32, (i - base) as u32, 1, &mut compiti);
        }
        for c in &compiti {
            let orig = piani(c.indice);
            let a = st.riferimento(c.rif_a).expect("riferimento a");
            let b = st.riferimento(c.rif_b.expect("riferimento b")).expect("riferimento b");
            let (passo, croma) = (passo_a(passo, c.profondita), passo_a(croma, c.profondita));
            // la griglia si sceglie sul primo riferimento; verso il secondo si cerca sulla stessa griglia
            let sc = scegli_griglia(st, &orig.y, &a.y, cerca, passo, p.celle_mobili);
            let (geo, sa) = (sc.geo, sc.scarti);
            let va = sc.va;
            let (vb, sb) = cerca_vettori(&orig.y, &b.y, &geo, &candidati(st, &geo.griglia), cerca, passo);
            // per cella: passato, futuro o media, quello con lo scarto minore
            let n = geo.griglia.celle();
            let mut modi = vec![0u8; n];
            for cc in 0..n {
                let (ax, ay) = va[cc];
                let (bx, by) = vb[cc];
                let mut sm = 0i64;
                for &k in &geo.pixel[cc] {
                    let (x, y) = ((k as usize % w) as i32, (k as usize / w) as i32);
                    let m = (campione(&a.y, x, y, ax, ay) + campione(&b.y, x, y, bx, by) + 1) >> 1;
                    sm += (orig.y.v[k as usize] - m).unsigned_abs() as i64;
                }
                modi[cc] = if sm < sa[cc].min(sb[cc]) {
                    2
                } else if sb[cc] < sa[cc] {
                    1
                } else {
                    0
                };
            }
            let mut va = va;
            let mut vb = vb;
            let usa_a: Vec<bool> = modi.iter().map(|&m| m != 1).collect();
            let usa_b: Vec<bool> = modi.iter().map(|&m| m != 0).collect();
            azzera_inutili(&geo.griglia, &mut va, &usa_a);
            azzera_inutili(&geo.griglia, &mut vb, &usa_b);
            out.push(Fotogramma::Previsto(codifica_previsto(st, &orig, c, geo, sc.divisioni, va, vb, modi, passo, croma, livelli, cerca)));
        }
        ancora = i;
        avanza(out.len());
        // prossima àncora: a distanza `gruppo`, oppure la prossima chiave programmata se viene prima (così la
        // chiave cade davvero ogni `chiave_ogni`, non alla prima àncora che la supera), oppure l'ultimo fotogramma
        let prossima_chiave = if p.chiave_ogni > 0 { (base / p.chiave_ogni as usize + 1) * p.chiave_ogni as usize } else { usize::MAX };
        let prossima = (i + gruppo).min(prossima_chiave);
        i = if prossima < n_tot { prossima } else if i < n_tot - 1 { n_tot - 1 } else { n_tot };
    }
    Ok(Reel { fotogrammi: out, fps: p.fps })
}

/// Come `codifica_reel_con`, su più fili: con `chiave_ogni` > 0 il reel si taglia in tratti da `chiave_ogni`
/// fotogrammi, ognuno parte da una chiave e non guarda gli altri, quindi si codificano insieme (un filo per tratto,
/// al massimo `fili`) e si rimettono in fila. Ogni tratto è un reel completo: è anche un pezzo per la rete
/// (`pacchetto::pezzi_reel`). L'ultimo fotogramma di ogni tratto diventa un'àncora (bidirezionali fino alla fine
/// del tratto) invece della coda in catena prima della chiave dopo. Con `chiave_ogni` = 0 o `fili` ≤ 1 è
/// `codifica_reel_con`. `avanza(fotogrammi codificati finora, in tutto)` arriva da più fili.
pub fn codifica_reel_parallela(fotogrammi: &[Immagine], p: ParametriReel, fili: usize, avanza: impl Fn(usize) + Sync) -> Result<Reel, Errore> {
    let tratto = p.chiave_ogni as usize;
    if tratto == 0 || fili <= 1 || fotogrammi.len() <= tratto {
        return codifica_reel_con(fotogrammi, p, avanza);
    }
    let primo = fotogrammi.first().ok_or(Errore::Immagine("nessun fotogramma"))?;
    if fotogrammi.iter().any(|f| f.larghezza != primo.larghezza || f.altezza != primo.altezza) {
        return Err(Errore::Immagine("fotogrammi di misure diverse"));
    }
    let tratti: Vec<&[Immagine]> = fotogrammi.chunks(tratto).collect();
    let prossimo = std::sync::atomic::AtomicUsize::new(0);
    let fatti = std::sync::atomic::AtomicUsize::new(0);
    let esiti: Vec<std::sync::Mutex<Option<Result<Reel, Errore>>>> = tratti.iter().map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..fili.min(tratti.len()) {
            s.spawn(|| loop {
                let t = prossimo.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(f) = tratti.get(t) else { break };
                let mut ultimo = 0;
                let r = codifica_reel_con(f, p, |n| {
                    let tot = fatti.fetch_add(n - ultimo, std::sync::atomic::Ordering::Relaxed) + n - ultimo;
                    ultimo = n;
                    avanza(tot);
                });
                *esiti[t].lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
            });
        }
    });
    let mut out = Vec::with_capacity(fotogrammi.len());
    for e in esiti {
        let r = e.into_inner().unwrap_or_else(|e| e.into_inner()).ok_or(Errore::Immagine("tratto non codificato"))??;
        out.extend(r.fotogrammi);
    }
    Ok(Reel { fotogrammi: out, fps: p.fps })
}

/// Codifica un previsto (semplice o bidirezionale) dato lo stato, l'originale, i riferimenti e gli spostamenti;
/// aggiorna lo stato con la ricostruzione.
#[allow(clippy::too_many_arguments)]
fn codifica_previsto(st: &mut Stato, orig: &Piani, c: &Compito, geo: Geometria, divisioni: Vec<u8>, va: Vec<(i32, i32)>, vb: Vec<(i32, i32)>, modi: Vec<u8>, passo: u8, croma: u8, livelli: u8, cerca: i32) -> Previsto {
    let n = geo.griglia.celle();
    let pred = {
        let a = st.riferimento(c.rif_a).expect("riferimento a");
        let b = c.rif_b.map(|m| st.riferimento(m).expect("riferimento b"));
        prevedi(&geo, &Previsione { a, b, va: &va, vb: &vb, modi: &modi })
    };
    // chi legge parte dagli stessi contesti: si codifica con una copia e si rilegge con l'altra
    let mut ctx_lettura = st.ctx.clone();
    let dettaglio = if passo > 0 { dettaglio::codifica(orig, &pred, passo, croma, livelli, &geo.mappa, n, Base::Tutto, &mut st.ctx) } else { vec![] };
    // il riferimento per i fotogrammi dopo è quello che vede chi legge, non l'originale
    let fine = if passo > 0 { dettaglio::decodifica(&dettaglio, &pred, passo, croma, livelli, &geo.mappa, n, Base::Tutto, &mut ctx_lettura) } else { pred };
    let piatti = |v: &[(i32, i32)]| v.iter().flat_map(|&(a, b)| [a, b]).collect::<Vec<i32>>();
    let vettori = strati::codifica_valori(&geo.griglia, &piatti(&va), 2);
    let (vettori_b, modi_b) = if c.rif_b.is_some() { (strati::codifica_valori(&geo.griglia, &piatti(&vb), 2), codifica_modi(&modi)) } else { (vec![], vec![]) };
    let mut testa = vec![passo, croma, livelli, cerca as u8];
    scrivi_varint(&mut testa, c.mostra as u64);
    scrivi_varint(&mut testa, c.rif_a as u64);
    if let Some(b) = c.rif_b {
        scrivi_varint(&mut testa, b as u64);
    }
    st.tieni(c.mostra, fine);
    st.geo = geo;
    st.vettori = campo_di_moto(&va, &vb, &modi, &TestaPrevisto { passo, croma, livelli, cerca: cerca as u8, mostra: c.mostra, rif_a: c.rif_a, rif_b: c.rif_b });
    Previsto { testa, divisioni, modi: modi_b, vettori, vettori_b, dettaglio }
}

/// Quello che dice l'intestazione di un previsto.
struct TestaPrevisto {
    passo: u8,
    croma: u8,
    livelli: u8,
    cerca: u8,
    mostra: u32,
    rif_a: u32,
    rif_b: Option<u32>,
}

fn leggi_testa_previsto(p: &Previsto) -> Result<TestaPrevisto, Errore> {
    if p.testa.len() < 6 {
        return Err(Errore::Formato("intestazione del fotogramma previsto"));
    }
    let (passo, croma, livelli, cerca) = (p.testa[0], p.testa[1], p.testa[2], p.testa[3]);
    if !(1..=LIVELLI_MAX).contains(&livelli) || cerca > CERCA_MAX {
        return Err(Errore::Formato("livelli o raggio di ricerca"));
    }
    let mut i = 4;
    let mostra = leggi_varint(&p.testa, &mut i)? as u32;
    let rif_a = leggi_varint(&p.testa, &mut i)? as u32;
    let rif_b = if p.bidirezionale() { Some(leggi_varint(&p.testa, &mut i)? as u32) } else { None };
    Ok(TestaPrevisto { passo, croma, livelli, cerca, mostra, rif_a, rif_b })
}

/// La griglia di un previsto: il reticolo della chiave con le divisioni previste dall'ultimo fotogramma, corrette
/// dai bit di `divisioni`.
fn griglia_del_previsto(base: &Griglia, ultima: &Griglia, vettori: &[(i32, i32)], p: &Previsto) -> Griglia {
    let previste = raffinate_previste(ultima, vettori);
    base.clone().con_raffinate(decodifica_divisioni(&p.divisioni, &previste))
}

/// Gli spostamenti di un previsto, per cella, verso il primo e il secondo riferimento, e i modi.
fn leggi_vettori(g: &Griglia, p: &Previsto, t: &TestaPrevisto) -> (Vec<(i32, i32)>, Vec<(i32, i32)>, Vec<u8>) {
    let lim = 2 * t.cerca as i32;
    let coppie = |b: &[u8]| strati::decodifica_valori(g, b, 2, -lim, lim).chunks_exact(2).map(|c| (c[0], c[1])).collect::<Vec<_>>();
    let va = coppie(&p.vettori);
    if t.rif_b.is_some() {
        (va, coppie(&p.vettori_b), decodifica_modi(&p.modi, g.celle()))
    } else {
        (va, vec![], vec![])
    }
}

/// Decodifica tutti i fotogrammi, in ordine di **visualizzazione**. Un previsto con il dettaglio a metà o vuoto
/// esce intero, meno fine.
pub fn decodifica_reel(r: &Reel) -> Result<Vec<Immagine>, Errore> {
    let mut stato: Option<Stato> = None;
    let mut base = 0usize;
    let mut out: Vec<(usize, Immagine)> = Vec::with_capacity(r.fotogrammi.len());
    for f in &r.fotogrammi {
        match f {
            Fotogramma::Chiave(c) => {
                let (rif, g) = decodifica_piani(c, Strati::Tutti)?;
                base = out.len();
                out.push((base, piani_in_rgb(&rif)));
                stato = Some(Stato::nuovo(g, rif));
            }
            Fotogramma::Previsto(p) => {
                let st = stato.as_mut().ok_or(Errore::Formato("fotogramma previsto senza una chiave prima"))?;
                let t = leggi_testa_previsto(p)?;
                let geo = st.geometria_per(griglia_del_previsto(&st.base, &st.geo.griglia, &st.vettori, p).raffinate());
                let (va, vb, modi) = leggi_vettori(&geo.griglia, p, &t);
                let pred = {
                    let a = st.riferimento(t.rif_a).ok_or(Errore::Formato("riferimento mancante"))?;
                    let b = match t.rif_b {
                        Some(m) => Some(st.riferimento(m).ok_or(Errore::Formato("secondo riferimento mancante"))?),
                        None => None,
                    };
                    prevedi(&geo, &Previsione { a, b, va: &va, vb: &vb, modi: &modi })
                };
                let fine = if t.passo > 0 && !p.dettaglio.is_empty() { dettaglio::decodifica(&p.dettaglio, &pred, t.passo, t.croma, t.livelli, &geo.mappa, geo.griglia.celle(), Base::Tutto, &mut st.ctx) } else { pred };
                out.push((base + t.mostra as usize, piani_in_rgb(&fine)));
                st.tieni(t.mostra, fine);
                st.geo = geo;
                st.vettori = campo_di_moto(&va, &vb, &modi, &t);
            }
        }
    }
    out.sort_by_key(|(i, _)| *i);
    Ok(out.into_iter().map(|(_, img)| img).collect())
}

impl Reel {
    pub fn byte_totali(&self) -> usize {
        self.fotogrammi.iter().map(|f| f.byte_totali()).sum()
    }

    /// Ripercorre i fotogrammi fino all'`i`-esimo (in ordine di decodifica) seguendo solo griglie e spostamenti,
    /// senza toccare i pixel: la griglia di un previsto dipende dagli spostamenti di quello prima. Restituisce la
    /// griglia del fotogramma `i` e i suoi spostamenti verso il primo riferimento (vuoti per una chiave).
    fn ripercorri(&self, i: usize) -> Result<(Griglia, Vec<(i32, i32)>, Vec<u8>), Errore> {
        let mut corrente: Option<(Griglia, Griglia, Vec<(i32, i32)>)> = None; // base, ultima griglia, ultimi spostamenti
        for f in &self.fotogrammi[..=i] {
            match f {
                Fotogramma::Chiave(c) => {
                    let g = crate::leggi_testa(&c.testa)?.griglia;
                    let v = vec![(0, 0); g.celle()];
                    corrente = Some((g.clone(), g, v));
                }
                Fotogramma::Previsto(p) => {
                    let (base, ultima, vettori) = corrente.take().ok_or(Errore::Formato("fotogramma previsto senza una chiave prima"))?;
                    let g = griglia_del_previsto(&base, &ultima, &vettori, p);
                    let t = leggi_testa_previsto(p)?;
                    let (va, vb, modi) = leggi_vettori(&g, p, &t);
                    if std::ptr::eq(f, &self.fotogrammi[i]) {
                        return Ok((g, va, modi));
                    }
                    let moto = campo_di_moto(&va, &vb, &modi, &t);
                    corrente = Some((base, g, moto));
                }
            }
        }
        let (_, g, _) = corrente.ok_or(Errore::Formato("fotogramma inesistente"))?;
        Ok((g, vec![], vec![]))
    }

    /// La griglia del fotogramma `i` (in ordine di decodifica): per misurare e guardare quante celle ha e quali sono
    /// divise.
    pub fn griglia(&self, i: usize) -> Result<Griglia, Errore> {
        self.fotogrammi.get(i).ok_or(Errore::Formato("fotogramma inesistente"))?;
        Ok(self.ripercorri(i)?.0)
    }

    /// Gli spostamenti per cella verso il primo riferimento del fotogramma `i` (in ordine di decodifica), in mezzi
    /// pixel (vuoto per una chiave): per misurare e guardare.
    pub fn vettori(&self, i: usize) -> Result<Vec<(i32, i32)>, Errore> {
        self.fotogrammi.get(i).ok_or(Errore::Formato("fotogramma inesistente"))?;
        Ok(self.ripercorri(i)?.1)
    }

    /// Per cella da dove si prevede il fotogramma `i` (0 passato, 1 futuro, 2 media); vuoto se non bidirezionale.
    pub fn modi(&self, i: usize) -> Result<Vec<u8>, Errore> {
        self.fotogrammi.get(i).ok_or(Errore::Formato("fotogramma inesistente"))?;
        Ok(self.ripercorri(i)?.2)
    }

    /// Tutto il reel in un elenco di byte: magia, versione, fotogrammi al secondo, numero di fotogrammi, poi per
    /// ognuno il tipo e i suoi pezzi con la lunghezza davanti.
    pub fn in_byte(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(self.byte_totali() + 16 * self.fotogrammi.len());
        b.extend_from_slice(MAGIA_REEL);
        b.push(VERSIONE_REEL);
        b.push(self.fps.max(1));
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
                    for q in [&p.testa, &p.divisioni, &p.modi, &p.vettori, &p.vettori_b, &p.dettaglio] {
                        pezzo(&mut b, q);
                    }
                }
            }
        }
        b
    }

    pub fn da_byte(b: &[u8]) -> Result<Reel, Errore> {
        if b.len() < 7 || &b[0..4] != MAGIA_REEL {
            return Err(Errore::Formato("intestazione del reel"));
        }
        if b[4] != VERSIONE_REEL {
            return Err(Errore::Formato("versione del reel sconosciuta"));
        }
        let fps = b[5];
        if fps == 0 {
            return Err(Errore::Formato("fotogrammi al secondo"));
        }
        let mut i = 6;
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
                    let (testa, divisioni, modi, vettori, vettori_b, dettaglio) = (pezzo(&mut i)?, pezzo(&mut i)?, pezzo(&mut i)?, pezzo(&mut i)?, pezzo(&mut i)?, pezzo(&mut i)?);
                    fotogrammi.push(Fotogramma::Previsto(Previsto { testa, divisioni, modi, vettori, vettori_b, dettaglio }));
                }
                _ => return Err(Errore::Formato("tipo di fotogramma sconosciuto")),
            }
        }
        Ok(Reel { fotogrammi, fps })
    }
}

/// Scene sintetiche per le prove (anche di `pacchetto`).
#[cfg(test)]
pub(crate) mod prova {
    use crate::Immagine;

    /// Una scena con texture (scacchi da 16 pixel con una trama fine sopra, colori che cambiano piano) vista da
    /// `(sx, sy)`: spostare la finestra è come muovere la camera.
    pub fn scena(w: u32, h: u32, sx: i32, sy: i32, fase: i32) -> Immagine {
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
}

#[cfg(test)]
mod tests {
    use super::prova::scena;
    use super::*;
    use crate::psnr_x100;

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
        ParametriReel { foto: Parametri { raggio: 12, ..Parametri::default() }, chiave_ogni: 0, cerca: 8, soglia_chiave: 40, bidirezionali: 0, celle_mobili: true, fps: 24 }
    }

    fn tipi(r: &Reel) -> String {
        r.fotogrammi
            .iter()
            .map(|f| match f {
                Fotogramma::Chiave(_) => 'C',
                Fotogramma::Previsto(p) if p.bidirezionale() => 'B',
                Fotogramma::Previsto(_) => 'P',
            })
            .collect()
    }

    #[test]
    fn reel_fermo_costa_quasi_niente() {
        let f: Vec<Immagine> = (0..5).map(|_| scena(192, 128, 0, 0, 0)).collect();
        let r = codifica_reel(&f, par()).unwrap();
        assert_eq!(tipi(&r), "CPPPP");
        let chiave = r.fotogrammi[0].byte_totali();
        for p in &r.fotogrammi[1..] {
            assert!(p.byte_totali() * 20 < chiave, "fermo: previsto {} byte contro chiave {}", p.byte_totali(), chiave);
        }
        let d = decodifica_reel(&r).unwrap();
        assert_eq!(d.len(), 5);
        assert!(psnr_minimo(&f, &d) >= 3000, "PSNR {}", psnr_minimo(&f, &d));
        // quasi tutte ferme: qualche cella può preferire il mezzo pixel, che liscia l'errore di quantizzazione
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
    fn bidirezionali_a_piramide_tornano_in_ordine_e_costano_meno() {
        // tra un'àncora e l'altra (distanza 4) la scena si sposta di (12, −8) pixel: serve un raggio di ricerca più largo
        let f: Vec<Immagine> = (0..9).map(|i| scena(192, 128, 3 * i, -2 * i, 0)).collect();
        let par = || ParametriReel { cerca: 16, ..par() };
        let r = codifica_reel(&f, ParametriReel { bidirezionali: 3, ..par() }).unwrap();
        // chiave, àncora 4, poi 2, 1, 3; àncora 8, poi 6, 5, 7
        assert_eq!(tipi(&r), "CPBBBPBBB");
        // il mezzo e i quarti hanno il passo più grosso dell'àncora (3/8 e 6/8 in più)
        let passo_di = |k: usize| if let Fotogramma::Previsto(p) = &r.fotogrammi[k] { p.testa[0] } else { 0 };
        assert_eq!((passo_di(1), passo_di(2), passo_di(3), passo_di(4)), (8, 11, 14, 14));
        let d = decodifica_reel(&r).unwrap();
        assert_eq!(d.len(), 9);
        assert!(psnr_minimo(&f, &d) >= 2800, "PSNR {}", psnr_minimo(&f, &d));
        // in ordine di visualizzazione: ogni decodificato somiglia al suo originale più che al vicino
        for i in 1..9 {
            assert!(psnr_x100(&f[i], &d[i]).unwrap() > psnr_x100(&f[i - 1], &d[i]).unwrap(), "fotogramma {i} fuori posto");
        }
        let modi = r.modi(2).unwrap();
        assert_eq!(modi.len(), r.vettori(2).unwrap().len());
        let semplice = codifica_reel(&f, par()).unwrap();
        assert!(r.byte_totali() < semplice.byte_totali(), "bidirezionali {} byte, semplici {}", r.byte_totali(), semplice.byte_totali());
        // un gruppo che non torna: 6 fotogrammi con piramide di 4 → l'ultimo fa da àncora
        let f6: Vec<Immagine> = f[..6].to_vec();
        let r6 = codifica_reel(&f6, ParametriReel { bidirezionali: 3, ..par() }).unwrap();
        assert_eq!(tipi(&r6), "CPBBBP");
        assert_eq!(decodifica_reel(&r6).unwrap().len(), 6);
    }

    /// Due moti nella stessa scena: la metà sinistra scorre di (3, 0) a ogni fotogramma, la destra sta ferma.
    fn due_moti(w: u32, h: u32, i: i32) -> Immagine {
        let sinistra = scena(w, h, 3 * i, 0, 0);
        let destra = scena(w, h, 0, 0, 1);
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h as usize {
            for x in 0..w as usize {
                let k = (y * w as usize + x) * 3;
                rgb.extend_from_slice(if x < w as usize / 2 { &sinistra.rgb[k..k + 3] } else { &destra.rgb[k..k + 3] });
            }
        }
        Immagine { larghezza: w, altezza: h, rgb }
    }

    #[test]
    fn le_celle_si_dividono_dove_il_moto_cambia_e_chi_legge_lo_prevede() {
        let f: Vec<Immagine> = (0..6).map(|i| due_moti(192, 128, i)).collect();
        let r = codifica_reel(&f, par()).unwrap();
        assert_eq!(tipi(&r), "CPPPPP");
        let base = r.griglia(0).unwrap().celle_base();
        // il primo previsto parte dalla griglia prevista (tutta intera: la chiave era ferma) e divide le celle sul confine
        let g1 = r.griglia(1).unwrap();
        let divise1 = g1.raffinate().iter().filter(|&&d| d).count();
        assert!(divise1 > 0 && divise1 * 2 < base, "divise {divise1} su {base}");
        // le celle divise stanno sul confine tra i due moti (x ≈ 96): nessuna lontana
        for k in 0..base {
            if g1.raffinate()[k] {
                let (cx, _) = g1.centro(g1.inizio[k]);
                assert!((cx / S - 96).abs() <= 2 * 12, "cella divisa lontana dal confine: x = {}", cx / S);
            }
        }
        // dal secondo previsto in poi chi legge prevede le divisioni dagli spostamenti: i bit di correzione calano
        let div = |i: usize| if let Fotogramma::Previsto(p) = &r.fotogrammi[i] { p.divisioni.len() } else { 0 };
        assert!(div(1) > 0, "il primo previsto deve dire quali celle divide");
        assert!(div(2) < div(1) && div(3) < div(1), "correzioni {} {} {}", div(1), div(2), div(3));
        let g3 = r.griglia(3).unwrap();
        assert!(g3.raffinate().iter().filter(|&&d| d).count() > 0);
        // si decodifica uguale e costa meno che con le celle fisse
        let d = decodifica_reel(&r).unwrap();
        assert!(psnr_minimo(&f, &d) >= 2800, "PSNR {}", psnr_minimo(&f, &d));
        let fisse = codifica_reel(&f, ParametriReel { celle_mobili: false, ..par() }).unwrap();
        assert!(r.griglia(3).unwrap().celle() > fisse.griglia(3).unwrap().celle());
        let dfisse = decodifica_reel(&fisse).unwrap();
        assert!(r.byte_totali() < fisse.byte_totali() || psnr_minimo(&f, &d) > psnr_minimo(&f, &dfisse) + 50, "mobili {} byte, fisse {}", r.byte_totali(), fisse.byte_totali());
    }

    #[test]
    fn cambio_di_scena_fa_una_chiave() {
        let f = vec![scena(160, 96, 0, 0, 0), scena(160, 96, 0, 0, 0), gradiente(160, 96), gradiente(160, 96)];
        let r = codifica_reel(&f, ParametriReel { soglia_chiave: 24, ..par() }).unwrap();
        assert_eq!(tipi(&r), "CPCP");
        // chiave forzata ogni 2
        let r2 = codifica_reel(&f, ParametriReel { chiave_ogni: 2, soglia_chiave: 0, ..par() }).unwrap();
        assert_eq!(tipi(&r2), "CPCP");
        // con i bidirezionali un cambio di scena su un àncora con fotogrammi in mezzo: quelli in mezzo escono in catena PRIMA della chiave (dal 16/11; prima si rifaceva tutto il reel senza bidirezionali)
        let f = vec![scena(160, 96, 0, 0, 0), scena(160, 96, 1, 0, 0), scena(160, 96, 2, 0, 0), scena(160, 96, 3, 0, 0), gradiente(160, 96), gradiente(160, 96)];
        let r3 = codifica_reel(&f, ParametriReel { soglia_chiave: 24, bidirezionali: 3, ..par() }).unwrap();
        assert_eq!(tipi(&r3), "CPPPCP");
        assert_eq!(decodifica_reel(&r3).unwrap().len(), 6);
    }

    #[test]
    fn in_byte_e_ritorno_troncature_e_spazzatura() {
        let f: Vec<Immagine> = (0..5).map(|i| scena(128, 96, 2 * i, i, 0)).collect();
        let r = codifica_reel(&f, ParametriReel { bidirezionali: 3, ..par() }).unwrap();
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
        assert_eq!(d.len(), 5);
        assert!(psnr_x100(&f[3], &d[3]).unwrap() >= 2000);
        // byte a caso dentro modi, vettori e dettaglio: errore o immagine, mai panico
        let mut rotto = r.clone();
        if let Fotogramma::Previsto(p) = &mut rotto.fotogrammi[2] {
            for (i, x) in p.vettori.iter_mut().enumerate() {
                *x ^= (i as u8).wrapping_mul(97);
            }
            for (i, x) in p.modi.iter_mut().enumerate() {
                *x = x.wrapping_add((i as u8).wrapping_mul(31));
            }
            p.divisioni = vec![0x5a, 0xa5, 0x00, 0xff, 0x13];
            for (i, x) in p.dettaglio.iter_mut().enumerate() {
                *x = x.wrapping_add(i as u8);
            }
        }
        let _ = decodifica_reel(&rotto);
        // un previsto senza chiave prima è un errore; un riferimento che non c'è pure
        let orfano = Reel { fotogrammi: vec![r.fotogrammi[1].clone()], fps: 24 };
        assert!(decodifica_reel(&orfano).is_err());
        let senza_ancora = Reel { fotogrammi: vec![r.fotogrammi[0].clone(), r.fotogrammi[2].clone()], fps: 24 };
        assert!(decodifica_reel(&senza_ancora).is_err());
    }

    #[test]
    fn a_tratti_su_piu_fili_e_un_reel_valido_e_sempre_lo_stesso() {
        let f: Vec<Immagine> = (0..22).map(|i| scena(96, 64, 2 * i, i / 2, 0)).collect();
        let p = ParametriReel { chiave_ogni: 6, bidirezionali: 3, fps: 12, ..par() };
        // un filo solo (o niente chiavi programmate): identico alla codifica di sempre
        let uno = codifica_reel_con(&f, p, |_| {}).unwrap();
        assert_eq!(codifica_reel_parallela(&f, p, 1, |_| {}).unwrap().in_byte(), uno.in_byte());
        let p0 = ParametriReel { chiave_ogni: 0, ..p };
        assert_eq!(codifica_reel_parallela(&f, p0, 4, |_| {}).unwrap().in_byte(), codifica_reel(&f, p0).unwrap().in_byte());
        // più fili: stesso risultato qualunque sia il numero di fili (l'ordine dei tratti non dipende da chi finisce prima)
        let fatti = std::sync::atomic::AtomicUsize::new(0);
        let due = codifica_reel_parallela(&f, p, 2, |n| { fatti.fetch_max(n, std::sync::atomic::Ordering::Relaxed); }).unwrap();
        assert_eq!(fatti.into_inner(), 22, "l'avanzamento arriva a tutti i fotogrammi");
        let quattro = codifica_reel_parallela(&f, p, 4, |_| {}).unwrap();
        assert_eq!(due.in_byte(), quattro.in_byte());
        // ogni tratto parte da una chiave: un pezzo per tratto, e pezzo per pezzo si vede lo stesso reel
        let tipi_: String = due.fotogrammi.iter().map(|x| if matches!(x, Fotogramma::Chiave(_)) { 'C' } else { '.' }).collect();
        assert_eq!(tipi_.match_indices('C').map(|(i, _)| i).collect::<Vec<_>>(), vec![0, 6, 12, 18]);
        let tutti = decodifica_reel(&due).unwrap();
        assert_eq!(tutti.len(), 22);
        let mut visti = Vec::new();
        for pz in crate::pacchetto::pezzi_reel(&due) {
            visti.extend(crate::pacchetto::apri_reel(&pz).unwrap().0);
        }
        assert_eq!(visti, tutti);
        assert!(psnr_minimo(&f, &tutti) > 3000, "qualità come a un filo: {}", psnr_minimo(&f, &tutti));
        // misure diverse si rifiutano anche a tratti
        let mut storti = f.clone();
        storti[13] = scena(64, 64, 0, 0, 0);
        assert!(codifica_reel_parallela(&storti, p, 4, |_| {}).is_err());
    }
}
