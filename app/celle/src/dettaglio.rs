//! Strato 2, il dettaglio: la luce (intera) e i due colori (dimezzati) passano per la trasformata a onde
//! (`onde`), le bande si quantizzano con una zona morta e si scrivono con la codifica aritmetica (`aritmetica`),
//! dalla banda più grossolana alla più fine, a **pezzi**: ogni pezzo è un gruppo di bande con la sua lunghezza
//! davanti, così uno strato troncato dà le bande intere che ci sono e il resto resta "come prima".
//!
//! Che cosa si codifica. Gli strati 0 e 1 (colore e pendenza per cella) danno un'immagine di base `B`. Il
//! decodificatore trasforma anche `B` e usa la sua **media grossolana** (la banda LL) come previsione: lo strato 2
//! porta solo la differenza di LL (quasi niente) e le bande alte dell'**immagine vera**. Le bande alte di `B` non si
//! usano: ai bordi delle celle `B` ha dei gradini che l'immagine vera non ha, e codificarne la differenza costerebbe
//! byte per cancellarli (misurato, vedi README).
//!
//! Dove non c'è dettaglio i coefficienti sono zero e costano una frazione di bit ciascuno: i byte vanno da soli nelle
//! zone ricche, cella per cella, senza decidere niente a mano.

use crate::aritmetica::{Lettore, Prob, Scrittore};
use crate::onde::{self, Banda, Orient};
use crate::{leggi_varint, scrivi_varint};

pub const LIVELLI_MAX: u8 = 5;
/// Un coefficiente quantizzato `q` si ricostruisce a `(|q| + RIC/8)·passo`: un po' sotto la metà dell'intervallo,
/// perché i valori piccoli sono più frequenti dei grandi.
const RIC: i64 = 3;
/// Per i resti di un reel (`Base::Tutto`) un po' più in basso: 2/8 (misurato il 13/11: −0,6 % di byte, +0,05 dB).
const RIC_RESTO: i64 = 2;

/// Un piano di interi, righe dall'alto.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Piano {
    pub w: usize,
    pub h: usize,
    pub v: Vec<i32>,
}

impl Piano {
    pub fn nuovo(w: usize, h: usize) -> Self {
        Self { w, h, v: vec![0; w * h] }
    }
    #[inline]
    fn a(&self, x: usize, y: usize) -> i32 {
        self.v[y * self.w + x]
    }
}

/// Luce intera, colori dimezzati.
#[derive(Clone, Debug)]
pub struct Piani {
    pub y: Piano,
    pub cb: Piano,
    pub cr: Piano,
}

/// Media 2×2 (ai bordi si ripete l'ultimo pixel).
pub fn dimezza(p: &Piano) -> Piano {
    let (w, h) = (p.w.div_ceil(2), p.h.div_ceil(2));
    let mut out = Piano::nuovo(w, h);
    for y in 0..h {
        let (y0, y1) = (2 * y, (2 * y + 1).min(p.h - 1));
        for x in 0..w {
            let (x0, x1) = (2 * x, (2 * x + 1).min(p.w - 1));
            out.v[y * w + x] = (p.a(x0, y0) + p.a(x1, y0) + p.a(x0, y1) + p.a(x1, y1) + 2) >> 2;
        }
    }
    out
}

/// Da dimezzato a intero con pesi 9/3/3/1 (come il "fancy upsampling" dei JPEG).
pub fn raddoppia(p: &Piano, w: usize, h: usize) -> Piano {
    let mut out = Piano::nuovo(w, h);
    for y in 0..h {
        let cy = (y / 2).min(p.h - 1);
        let ny = if y % 2 == 0 { cy.saturating_sub(1) } else { (cy + 1).min(p.h - 1) };
        for x in 0..w {
            let cx = (x / 2).min(p.w - 1);
            let nx = if x % 2 == 0 { cx.saturating_sub(1) } else { (cx + 1).min(p.w - 1) };
            out.v[y * w + x] = (9 * p.a(cx, cy) + 3 * p.a(nx, cy) + 3 * p.a(cx, ny) + p.a(nx, ny) + 8) >> 4;
        }
    }
    out
}

// ─── Quantizzazione ─────────────────────────────────────────────────────────

/// Peso del passo per banda, in 64esimi: le bande più grossolane pesano di più sull'immagine (la sintesi 5/3
/// amplifica di ~1,5 per livello), quindi hanno un passo più fino; la diagonale (HH) ha un passo più largo.
fn peso64(b: &Banda) -> i64 {
    let (base, e) = match b.orient {
        Orient::LL => (66i64, b.livello as u32),
        Orient::HH => (92, b.livello.saturating_sub(1) as u32),
        _ => (64, b.livello.saturating_sub(1) as u32),
    };
    base * (1i64 << e) / 3i64.pow(e)
}

#[inline]
fn passo_x64(passo: u8, b: &Banda) -> i64 {
    (passo as i64 * peso64(b)).max(64)
}

/// Zona morta larga un passo: sotto il passo è zero, poi `⌊|v|/passo⌋` col segno.
#[inline]
fn quantizza(v: i32, px64: i64) -> i32 {
    let a = (v.unsigned_abs() as i64 * 64 / px64) as i32;
    if v < 0 {
        -a
    } else {
        a
    }
}

#[inline]
fn dequantizza(q: i32, px64: i64, modo: Base) -> i32 {
    if q == 0 {
        return 0;
    }
    let ric = if modo == Base::Tutto { RIC_RESTO } else { RIC };
    let a = ((q.unsigned_abs() as i64 * px64 + px64 * ric / 8 + 32) / 64).min(1 << 20) as i32;
    if q < 0 {
        -a
    } else {
        a
    }
}

// ─── Contesti ───────────────────────────────────────────────────────────────

const ORIENT: usize = 4;
const LIV: usize = LIVELLI_MAX as usize;
const VIC: usize = 4;
const GEN: usize = 4;
const PREF: usize = 16;

/// Le probabilità adattive, divise per orientamento, livello e "quanto c'è intorno". Un reel le tiene da un
/// fotogramma all'altro (chi scrive e chi legge allo stesso modo): le statistiche del resto cambiano poco.
#[derive(Clone)]
pub struct Contesti {
    sig: Vec<Prob>,
    gt1: Vec<Prob>,
    gt2: Vec<Prob>,
    segno: Vec<Prob>,
    pref: Vec<Prob>,
    /// "questa cella a questo livello è tutta zero": per livello e per quanto c'era al livello prima
    vuota: Vec<Prob>,
}

impl Contesti {
    pub fn nuovi() -> Self {
        Self {
            sig: vec![Prob::NUOVA; ORIENT * LIV * VIC * GEN],
            gt1: vec![Prob::NUOVA; ORIENT * LIV * VIC * GEN],
            gt2: vec![Prob::NUOVA; ORIENT * LIV * VIC * GEN],
            segno: vec![Prob::NUOVA; ORIENT * 9],
            pref: vec![Prob::NUOVA; ORIENT * LIV * PREF],
            vuota: vec![Prob::NUOVA; LIV * 4 * 2],
        }
    }
}

#[inline]
pub(crate) fn secchio(somma: i64) -> usize {
    match somma {
        0 => 0,
        1..=2 => 1,
        3..=8 => 2,
        _ => 3,
    }
}

#[inline]
fn segno_di(v: i32) -> usize {
    match v.signum() {
        0 => 0,
        1 => 1,
        _ => 2,
    }
}

/// Indici dei contesti di un coefficiente: (orientamento, livello, vicini, genitore, segno sinistra, segno sopra).
pub(crate) struct Dove {
    pub o: usize,
    pub l: usize,
    pub vic: usize,
    pub gen: usize,
    pub sl: usize,
    pub su: usize,
}

impl Dove {
    #[inline]
    fn sig(&self) -> usize {
        ((self.o * LIV + self.l) * VIC + self.vic) * GEN + self.gen
    }
    #[inline]
    fn gt(&self) -> usize {
        ((self.o * LIV + self.l) * VIC + self.vic) * GEN + self.gen
    }
    #[inline]
    fn segno(&self) -> usize {
        self.o * 9 + self.sl * 3 + self.su
    }
    #[inline]
    fn pref(&self, k: usize) -> usize {
        (self.o * LIV + self.l) * PREF + k.min(PREF - 1)
    }
}

/// Il contesto di `(x, y)` dentro la banda `b` del piano quantizzato `q`: quanto c'è intorno tra i coefficienti già
/// scritti (sinistra, sopra, le due diagonali sopra, due passi a sinistra e due sopra) e quanto c'è intorno al
/// coefficiente "genitore" nella banda di un livello più in alto (lui e i suoi otto vicini, già tutti scritti).
#[inline]
fn contesto(q: &Piano, b: &Banda, gen: Option<&Banda>, x: usize, y: usize) -> Dove {
    let at = |xx: usize, yy: usize| q.a(b.x0 + xx, b.y0 + yy);
    let mut somma = 0i64;
    let (mut sl, mut su) = (0, 0);
    if x > 0 {
        let v = at(x - 1, y);
        somma += v.unsigned_abs() as i64;
        sl = segno_di(v);
        if x > 1 {
            somma += at(x - 2, y).unsigned_abs() as i64;
        }
    }
    if y > 0 {
        let v = at(x, y - 1);
        somma += v.unsigned_abs() as i64;
        su = segno_di(v);
        if x > 0 {
            somma += at(x - 1, y - 1).unsigned_abs() as i64;
        }
        if x + 1 < b.w {
            somma += at(x + 1, y - 1).unsigned_abs() as i64;
        }
        if y > 1 {
            somma += at(x, y - 2).unsigned_abs() as i64;
        }
    }
    let gen = match gen {
        Some(g) if g.w > 0 && g.h > 0 => {
            let (px, py) = ((x / 2).min(g.w - 1), (y / 2).min(g.h - 1));
            let mut s = 2 * q.a(g.x0 + px, g.y0 + py).unsigned_abs() as i64;
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (xx, yy) = (px as i64 + dx, py as i64 + dy);
                    if (dx != 0 || dy != 0) && xx >= 0 && yy >= 0 && (xx as usize) < g.w && (yy as usize) < g.h {
                        s += q.a(g.x0 + xx as usize, g.y0 + yy as usize).unsigned_abs() as i64;
                    }
                }
            }
            secchio(s)
        }
        _ => 0,
    };
    Dove { o: b.orient.indice(), l: (b.livello.clamp(1, LIVELLI_MAX) - 1) as usize, vic: secchio(somma), gen, sl, su }
}

pub(crate) fn scrivi_valore(sc: &mut Scrittore, c: &mut Contesti, d: &Dove, v: i32) {
    sc.bit(&mut c.sig[d.sig()], v != 0);
    if v == 0 {
        return;
    }
    sc.bit(&mut c.segno[d.segno()], v < 0);
    let a = v.unsigned_abs() as u64;
    sc.bit(&mut c.gt1[d.gt()], a > 1);
    if a == 1 {
        return;
    }
    sc.bit(&mut c.gt2[d.gt()], a > 2);
    if a == 2 {
        return;
    }
    // Exp-Golomb di a − 3: m = a − 2 ≥ 1, k = ⌊log₂ m⌋ in unario (adattivo), poi i k bit sotto (diretti)
    let m = a - 2;
    let k = (63 - m.leading_zeros()) as usize;
    for j in 0..k {
        sc.bit(&mut c.pref[d.pref(j)], true);
    }
    sc.bit(&mut c.pref[d.pref(k)], false);
    for j in (0..k).rev() {
        sc.bit_diretto((m >> j) & 1 == 1);
    }
}

pub(crate) fn leggi_valore(le: &mut Lettore, c: &mut Contesti, d: &Dove) -> i32 {
    if !le.bit(&mut c.sig[d.sig()]) {
        return 0;
    }
    let negativo = le.bit(&mut c.segno[d.segno()]);
    let a: u64 = if !le.bit(&mut c.gt1[d.gt()]) {
        1
    } else if !le.bit(&mut c.gt2[d.gt()]) {
        2
    } else {
        let mut k = 0usize;
        while k < 24 && le.bit(&mut c.pref[d.pref(k)]) {
            k += 1;
        }
        let mut m = 1u64;
        for _ in 0..k {
            m = (m << 1) | le.bit_diretto() as u64;
        }
        m + 2
    };
    let a = a.min(1 << 24) as i32;
    if negativo {
        -a
    } else {
        a
    }
}

// ─── Pezzi: per livello, e dentro il livello cella per cella ────────────────

/// Quanti gruppi di celle per livello: ogni gruppo è un pezzo che si può troncare. Le celle vanno in ordine di
/// quanto dettaglio hanno mostrato al livello prima (o nella media, per il primo), così i primi pezzi portano il
/// dettaglio dove c'è; l'ordine lo ricava anche il decodificatore, senza byte in più.
pub const GRUPPI: usize = 8;

/// Quanti livelli hanno i colori dimezzati: uno in meno della luce (stessa scala sull'immagine).
fn livelli_croma(livelli: u8) -> u8 {
    livelli.saturating_sub(1)
}

/// I coefficienti di una banda raggruppati per cella (indici lineari nel piano), dalla mappa pixel → cella:
/// il coefficiente `(x, y)` di una banda a scala `scala` sta al pixel `(x·scala + scala/2, y·scala + scala/2)`.
fn per_cella(b: &Banda, stride: usize, scala: usize, mappa: &[u32], w: usize, h: usize, celle: usize) -> Vec<Vec<u32>> {
    let mut v = vec![Vec::new(); celle];
    for y in 0..b.h {
        let py = (y * scala + scala / 2).min(h - 1);
        for x in 0..b.w {
            let px = (x * scala + scala / 2).min(w - 1);
            v[mappa[py * w + px] as usize].push(((b.y0 + y) * stride + b.x0 + x) as u32);
        }
    }
    v
}

/// Un livello della luce con le bande dei colori alla stessa scala: (indice di banda, coefficienti per cella).
struct Livello {
    bande_y: Vec<(usize, Vec<Vec<u32>>)>,
    bande_c: Vec<(usize, Vec<Vec<u32>>)>,
}

/// Tutto quello che serve per percorrere i coefficienti nello stesso ordine da una parte e dall'altra.
struct Geometria {
    bande_y: Vec<Banda>,
    bande_c: Vec<Banda>,
    gen_y: Vec<Option<Banda>>,
    gen_c: Vec<Option<Banda>>,
    /// la media della luce, per cella
    ll_y: Vec<Vec<u32>>,
    livelli: Vec<Livello>,
    celle: usize,
    /// larghezza del piano della luce e di quelli dei colori
    w: usize,
    wc: usize,
}

fn genitore(bande: &[Banda], b: &Banda) -> Option<Banda> {
    if b.orient == Orient::LL {
        return None;
    }
    bande.iter().find(|g| g.orient == b.orient && g.livello == b.livello + 1).copied()
}

fn geometria(w: usize, h: usize, wc: usize, hc: usize, livelli: u8, mappa: &[u32], celle: usize) -> Geometria {
    let lc = livelli_croma(livelli);
    let bande_y = onde::bande(w, h, livelli);
    let bande_c = onde::bande(wc, hc, lc);
    let gen_y = bande_y.iter().map(|b| genitore(&bande_y, b)).collect();
    let gen_c = bande_c.iter().map(|b| genitore(&bande_c, b)).collect();
    let ll_y = per_cella(&bande_y[0], w, 1 << livelli, mappa, w, h, celle);
    let mut liv = Vec::new();
    for l in (1..=livelli).rev() {
        let i = 1 + 3 * (livelli - l) as usize;
        let scala = 1usize << l;
        let by = (i..i + 3).map(|k| (k, per_cella(&bande_y[k], w, scala, mappa, w, h, celle))).collect();
        // la banda dei colori al livello l−1 (piano dimezzato) sta alla stessa scala sull'immagine
        let bc = if l >= 2 && l - 1 <= lc { (i..i + 3).map(|k| (k, per_cella(&bande_c[k], wc, scala, mappa, w, h, celle))).collect() } else { vec![] };
        liv.push(Livello { bande_y: by, bande_c: bc });
    }
    Geometria { bande_y, bande_c, gen_y, gen_c, ll_y, livelli: liv, celle, w, wc }
}

/// Chi percorre i coefficienti: chi scrive e chi legge fanno le stesse mosse.
trait Visita {
    fn inizio_pezzo(&mut self);
    /// Falso = fermarsi (chi legge non ha più byte).
    fn fine_pezzo(&mut self) -> bool;
    /// Un coefficiente del piano (0 luce, 1 Cb, 2 Cr), banda `banda`, indice lineare `idx`.
    fn coeff(&mut self, piano: u8, banda: usize, idx: u32);
    /// |q| della luce in `idx`, per com'è dalla parte di chi legge (già scritto o zero).
    fn assoluto_y(&self, idx: u32) -> i64;
    /// Prima dei coefficienti di una cella a un livello: vero = la cella è tutta zero e si salta (il "blocco non
    /// codificato" dei codec a pixel: un bit al posto di centinaia di zeri). Chi scrive lo decide guardando i
    /// coefficienti (`y`: indici della luce per banda, `c`: dei colori) e lo scrive; chi legge lo legge.
    fn cella_vuota(&mut self, ctx: usize, y: &[&[u32]], c: &[&[u32]]) -> bool;
}

fn percorri(geo: &Geometria, v: &mut dyn Visita) {
    // pezzo 0: le medie, in ordine di riga
    v.inizio_pezzo();
    for (piano, b) in [(0u8, &geo.bande_y[0]), (1, &geo.bande_c[0]), (2, &geo.bande_c[0])] {
        let stride = if piano == 0 { geo.w } else { geo.wc };
        for y in 0..b.h {
            for x in 0..b.w {
                v.coeff(piano, 0, ((b.y0 + y) * stride + b.x0 + x) as u32);
            }
        }
    }
    if !v.fine_pezzo() {
        return;
    }
    let mut prima: Vec<Vec<u32>> = geo.ll_y.clone();
    for (l, liv) in geo.livelli.iter().enumerate() {
        // celle in ordine di dettaglio mostrato al livello prima (stabile: a parità, ordine di cella)
        let energia: Vec<i64> = (0..geo.celle).map(|c| prima[c].iter().map(|&i| v.assoluto_y(i)).sum::<i64>()).collect();
        let mut en: Vec<(i64, usize)> = (0..geo.celle).map(|c| (-energia[c], c)).collect();
        en.sort();
        let ordine: Vec<usize> = en.into_iter().map(|e| e.1).collect();
        let mut vuota_prima = false;
        for g in 0..GRUPPI {
            let (da, a) = (g * geo.celle / GRUPPI, (g + 1) * geo.celle / GRUPPI);
            v.inizio_pezzo();
            for &c in &ordine[da..a] {
                let iy: Vec<&[u32]> = liv.bande_y.iter().map(|(_, pc)| pc[c].as_slice()).collect();
                let ic: Vec<&[u32]> = liv.bande_c.iter().map(|(_, pc)| pc[c].as_slice()).collect();
                let ctx = (l.min(LIV - 1) * 4 + secchio(energia[c])) * 2 + vuota_prima as usize;
                vuota_prima = v.cella_vuota(ctx, &iy, &ic);
                if vuota_prima {
                    continue;
                }
                for (bi, pc) in &liv.bande_y {
                    for &idx in &pc[c] {
                        v.coeff(0, *bi, idx);
                    }
                }
                for (bi, pc) in &liv.bande_c {
                    for piano in [1u8, 2] {
                        for &idx in &pc[c] {
                            v.coeff(piano, *bi, idx);
                        }
                    }
                }
            }
            if !v.fine_pezzo() {
                return;
            }
        }
        prima = (0..geo.celle).map(|c| liv.bande_y.iter().flat_map(|(_, pc)| pc[c].iter().copied()).collect()).collect();
    }
}

/// Posizioni (fine, in byte) dei pezzi interi presenti in `d`.
pub fn pezzi_byte(d: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < d.len() {
        let Ok(n) = leggi_varint(d, &mut i) else { break };
        let fine = i + n as usize;
        if fine > d.len() {
            break;
        }
        out.push(fine);
        i = fine;
    }
    out
}

fn trasforma(p: &Piani, livelli: u8) -> Piani {
    let mut t = p.clone();
    onde::avanti(&mut t.y.v, t.y.w, t.y.h, livelli);
    let lc = livelli_croma(livelli);
    onde::avanti(&mut t.cb.v, t.cb.w, t.cb.h, lc);
    onde::avanti(&mut t.cr.v, t.cr.w, t.cr.h, lc);
    t
}

/// La previsione di una banda. Per le foto (`Base::SoloMedia`) la media grossolana viene dalla base e le bande alte
/// da zero: prevedere anche le bande alte dalla base degli strati 0 e 1 costava il 30 % in più sulla luce (misurato
/// il 12/11/2026). Per i fotogrammi previsti di un reel (`Base::Tutto`) la base è il fotogramma prima spostato
/// cella per cella e si usa tutta.
#[inline]
fn previsto(base_t: &Piano, b: &Banda, x: usize, y: usize, modo: Base) -> i32 {
    if modo == Base::Tutto || b.orient == Orient::LL {

        base_t.a(x, y)
    } else {
        0
    }
}

/// Chi scrive: legge i contesti da `visto` (quello che chi legge ha già), non dal piano intero.
struct Scrive<'a> {
    geo: &'a Geometria,
    q: [&'a Piano; 3],
    visto: [Piano; 3],
    ctx: &'a mut [Contesti; 2],
    sc: Option<Scrittore>,
    out: Vec<u8>,
}

impl Visita for Scrive<'_> {
    fn inizio_pezzo(&mut self) {
        self.sc = Some(Scrittore::nuovo());
    }
    fn fine_pezzo(&mut self) -> bool {
        let b = self.sc.take().expect("pezzo aperto").chiudi();
        scrivi_varint(&mut self.out, b.len() as u64);
        self.out.extend_from_slice(&b);
        true
    }
    fn coeff(&mut self, piano: u8, banda: usize, idx: u32) {
        let p = piano as usize;
        let (bande, gen) = if p == 0 { (&self.geo.bande_y, &self.geo.gen_y) } else { (&self.geo.bande_c, &self.geo.gen_c) };
        let b = &bande[banda];
        let idx = idx as usize;
        let (x, y) = (idx % self.q[p].w - b.x0, idx / self.q[p].w - b.y0);
        let d = contesto(&self.visto[p], b, gen[banda].as_ref(), x, y);
        let v = self.q[p].v[idx];
        scrivi_valore(self.sc.as_mut().expect("pezzo aperto"), &mut self.ctx[p.min(1)], &d, v);
        self.visto[p].v[idx] = v;
    }
    fn assoluto_y(&self, idx: u32) -> i64 {
        self.visto[0].v[idx as usize].unsigned_abs() as i64
    }
    fn cella_vuota(&mut self, ctx: usize, y: &[&[u32]], c: &[&[u32]]) -> bool {
        let vuota = y.iter().all(|b| b.iter().all(|&i| self.q[0].v[i as usize] == 0)) && c.iter().all(|b| b.iter().all(|&i| self.q[1].v[i as usize] == 0 && self.q[2].v[i as usize] == 0));
        self.sc.as_mut().expect("pezzo aperto").bit(&mut self.ctx[0].vuota[ctx], vuota);
        vuota
    }
}

/// Chi legge: riempie i piani quantizzati pezzo per pezzo, finché ci sono byte.
struct Legge<'a> {
    geo: &'a Geometria,
    q: [Piano; 3],
    ctx: &'a mut [Contesti; 2],
    d: &'a [u8],
    i: usize,
    le: Option<Lettore<'a>>,
}

impl Visita for Legge<'_> {
    fn inizio_pezzo(&mut self) {
        self.le = None;
        let Ok(n) = leggi_varint(self.d, &mut self.i) else { return };
        let fine = self.i + n as usize;
        if fine > self.d.len() {
            return;
        }
        self.le = Some(Lettore::nuovo(&self.d[self.i..fine]));
        self.i = fine;
    }
    fn fine_pezzo(&mut self) -> bool {
        self.le.is_some()
    }
    fn coeff(&mut self, piano: u8, banda: usize, idx: u32) {
        let Some(le) = self.le.as_mut() else { return };
        let p = piano as usize;
        let (bande, gen) = if p == 0 { (&self.geo.bande_y, &self.geo.gen_y) } else { (&self.geo.bande_c, &self.geo.gen_c) };
        let b = &bande[banda];
        let idx = idx as usize;
        let (x, y) = (idx % self.q[p].w - b.x0, idx / self.q[p].w - b.y0);
        let d = contesto(&self.q[p], b, gen[banda].as_ref(), x, y);
        self.q[p].v[idx] = leggi_valore(le, &mut self.ctx[p.min(1)], &d);
    }
    fn assoluto_y(&self, idx: u32) -> i64 {
        self.q[0].v[idx as usize].unsigned_abs() as i64
    }
    fn cella_vuota(&mut self, ctx: usize, _y: &[&[u32]], _c: &[&[u32]]) -> bool {
        match self.le.as_mut() {
            Some(le) => le.bit(&mut self.ctx[0].vuota[ctx]),
            None => true,
        }
    }
}

/// Che cosa della base fa da previsione per il dettaglio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    /// Solo la media grossolana (banda LL): per le foto, dove la base viene dagli strati 0 e 1 e ha i gradini ai
    /// bordi delle celle.
    SoloMedia,
    /// Tutte le bande: per i fotogrammi di un reel, dove la base è il fotogramma prima spostato cella per cella e
    /// somiglia davvero all'immagine, bande alte comprese.
    Tutto,
}

/// Codifica lo strato 2. `orig` e `base` sono i piani dell'immagine vera e dell'immagine di base (luce intera,
/// colori dimezzati); `mappa` dice la cella di ogni pixel (`celle` in tutto); `modo` quanto della base si usa.
pub fn codifica(orig: &Piani, base: &Piani, passo: u8, passo_croma: u8, livelli: u8, mappa: &[u32], celle: usize, modo: Base, ctx: &mut [Contesti; 2]) -> Vec<u8> {
    let livelli = livelli.clamp(1, LIVELLI_MAX);
    let (orig_t, base_t) = (trasforma(orig, livelli), trasforma(base, livelli));
    let geo = geometria(orig.y.w, orig.y.h, orig.cb.w, orig.cb.h, livelli, mappa, celle);
    let quantizza_piano = |o: &Piano, b: &Piano, bande: &[Banda], passo: u8| {
        let mut q = Piano::nuovo(o.w, o.h);
        for banda in bande {
            let px = passo_x64(passo, banda);
            for y in banda.y0..banda.y0 + banda.h {
                for x in banda.x0..banda.x0 + banda.w {
                    q.v[y * o.w + x] = quantizza(o.a(x, y) - previsto(b, banda, x, y, modo), px);
                }
            }
        }
        q
    };
    let qy = quantizza_piano(&orig_t.y, &base_t.y, &geo.bande_y, passo);
    let qcb = quantizza_piano(&orig_t.cb, &base_t.cb, &geo.bande_c, passo_croma);
    let qcr = quantizza_piano(&orig_t.cr, &base_t.cr, &geo.bande_c, passo_croma);
    let mut s = Scrive {
        geo: &geo,
        q: [&qy, &qcb, &qcr],
        visto: [Piano::nuovo(qy.w, qy.h), Piano::nuovo(qcb.w, qcb.h), Piano::nuovo(qcr.w, qcr.h)],
        ctx,
        sc: None,
        out: Vec::new(),
    };
    percorri(&geo, &mut s);
    s.out
}

/// Decodifica lo strato 2 (anche a metà) sopra la base e restituisce i piani ricostruiti (luce intera, colori
/// dimezzati). Pezzi mancanti o troncati: le loro bande restano come previste.
pub fn decodifica(d: &[u8], base: &Piani, passo: u8, passo_croma: u8, livelli: u8, mappa: &[u32], celle: usize, modo: Base, ctx: &mut [Contesti; 2]) -> Piani {
    let livelli = livelli.clamp(1, LIVELLI_MAX);
    let base_t = trasforma(base, livelli);
    let lc = livelli_croma(livelli);
    let geo = geometria(base.y.w, base.y.h, base.cb.w, base.cb.h, livelli, mappa, celle);
    let mut l = Legge {
        geo: &geo,
        q: [Piano::nuovo(base.y.w, base.y.h), Piano::nuovo(base.cb.w, base.cb.h), Piano::nuovo(base.cb.w, base.cb.h)],
        ctx,
        d,
        i: 0,
        le: None,
    };
    percorri(&geo, &mut l);
    let [qy, qcb, qcr] = l.q;
    let ricostruisci = |q: &Piano, b: &Piano, bande: &[Banda], passo: u8, liv: u8| {
        let mut p = Piano::nuovo(q.w, q.h);
        for banda in bande {
            let px = passo_x64(passo, banda);
            for y in banda.y0..banda.y0 + banda.h {
                for x in banda.x0..banda.x0 + banda.w {
                    p.v[y * q.w + x] = previsto(b, banda, x, y, modo).saturating_add(dequantizza(q.a(x, y), px, modo));
                }
            }
        }
        onde::indietro(&mut p.v, p.w, p.h, liv);
        p
    };
    Piani {
        y: ricostruisci(&qy, &base_t.y, &geo.bande_y, passo, livelli),
        cb: ricostruisci(&qcb, &base_t.cb, &geo.bande_c, passo_croma, lc),
        cr: ricostruisci(&qcr, &base_t.cr, &geo.bande_c, passo_croma, lc),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nuovi() -> [Contesti; 2] {
        [Contesti::nuovi(), Contesti::nuovi()]
    }

    fn caso(stato: &mut u64) -> u64 {
        *stato ^= *stato << 13;
        *stato ^= *stato >> 7;
        *stato ^= *stato << 17;
        *stato
    }

    fn piani_casuali(w: usize, h: usize, stato: &mut u64, ampiezza: i32) -> Piani {
        let mut y = Piano::nuovo(w, h);
        for v in y.v.iter_mut() {
            *v = 128 + (caso(stato) % (2 * ampiezza as u64 + 1)) as i32 - ampiezza;
        }
        let mut c = Piano::nuovo(w.div_ceil(2), h.div_ceil(2));
        for v in c.v.iter_mut() {
            *v = 128 + (caso(stato) % (ampiezza as u64 + 1)) as i32 - ampiezza / 2;
        }
        Piani { y, cb: c.clone(), cr: c }
    }

    /// Tre celle: colonne di sinistra, centro, destra.
    fn mappa_prova(w: usize, h: usize) -> (Vec<u32>, usize) {
        ((0..w * h).map(|k| ((k % w) * 3 / w) as u32).collect(), 3)
    }

    fn piatti(w: usize, h: usize, v: i32) -> Piani {
        let mut y = Piano::nuovo(w, h);
        y.v.iter_mut().for_each(|p| *p = v);
        let mut c = Piano::nuovo(w.div_ceil(2), h.div_ceil(2));
        c.v.iter_mut().for_each(|p| *p = 128);
        Piani { y, cb: c.clone(), cr: c }
    }

    #[test]
    fn con_passo_uno_la_luce_torna_quasi_uguale_e_i_pezzi_sono_leggibili() {
        let mut stato = 7u64;
        for &(w, h, l) in &[(37usize, 23usize, 3u8), (8, 8, 1), (64, 48, 5), (5, 3, 2)] {
            let orig = piani_casuali(w, h, &mut stato, 40);
            let base = piatti(w, h, 128);
            let (mappa, celle) = mappa_prova(w, h);
            let d = codifica(&orig, &base, 1, 1, l, &mappa, celle, Base::SoloMedia, &mut nuovi());
            assert_eq!(pezzi_byte(&d).len(), 1 + GRUPPI * l as usize, "{w}x{h} livelli {l}");
            let r = decodifica(&d, &base, 1, 1, l, &mappa, celle, Base::SoloMedia, &mut nuovi());
            // passo 1 con zona morta: errore al massimo di un paio di livelli per coefficiente
            let max = orig.y.v.iter().zip(&r.y.v).map(|(a, b)| (a - b).abs()).max().unwrap();
            assert!(max <= 6, "{w}x{h} livelli {l}: errore massimo {max}");
            let max_c = orig.cb.v.iter().zip(&r.cb.v).map(|(a, b)| (a - b).abs()).max().unwrap();
            assert!(max_c <= 6, "colore: errore massimo {max_c}");
        }
    }

    #[test]
    fn un_piano_uguale_alla_base_costa_pochi_byte() {
        let base = piatti(200, 150, 90);
        let (mappa, celle) = mappa_prova(200, 150);
        let d = codifica(&base, &base, 8, 8, 3, &mappa, celle, Base::SoloMedia, &mut nuovi());
        // il costo è il "riscaldamento" delle probabilità dei contesti e la chiusura di 25 pezzi
        assert!(d.len() <= 400, "{} byte", d.len());
        let r = decodifica(&d, &base, 8, 8, 3, &mappa, celle, Base::SoloMedia, &mut nuovi());
        assert_eq!(r.y, base.y);
    }

    #[test]
    fn i_pezzi_migliorano_uno_dopo_l_altro_e_troncare_non_rompe() {
        let mut stato = 99u64;
        let orig = piani_casuali(64, 40, &mut stato, 60);
        let base = piatti(64, 40, 128);
        let (mappa, celle) = mappa_prova(64, 40);
        let d = codifica(&orig, &base, 4, 4, 3, &mappa, celle, Base::SoloMedia, &mut nuovi());
        let fini = pezzi_byte(&d);
        assert_eq!(fini.len(), 1 + 3 * GRUPPI);
        let errore = |p: &Piani| orig.y.v.iter().zip(&p.y.v).map(|(a, b)| ((a - b) * (a - b)) as u64).sum::<u64>();
        let mut prima = errore(&decodifica(&[], &base, 4, 4, 3, &mappa, celle, Base::SoloMedia, &mut nuovi()));
        for &f in &fini {
            let e = errore(&decodifica(&d[..f], &base, 4, 4, 3, &mappa, celle, Base::SoloMedia, &mut nuovi()));
            assert!(e <= prima, "un pezzo in più non peggiora: {prima} → {e}");
            prima = e;
        }
        // troncato a metà di un pezzo: vale come il pezzo prima; byte a caso: nessun panico
        let meta = decodifica(&d[..fini[1] + 3], &base, 4, 4, 3, &mappa, celle, Base::SoloMedia, &mut nuovi());
        assert_eq!(meta.y, decodifica(&d[..fini[1]], &base, 4, 4, 3, &mappa, celle, Base::SoloMedia, &mut nuovi()).y);
        let spazzatura: Vec<u8> = (0..300).map(|_| caso(&mut stato) as u8).collect();
        let _ = decodifica(&spazzatura, &base, 4, 4, 3, &mappa, celle, Base::SoloMedia, &mut nuovi());
    }

    #[test]
    fn dimezza_e_raddoppia_tornano_vicini() {
        let mut p = Piano::nuovo(31, 17);
        for (k, v) in p.v.iter_mut().enumerate() {
            *v = ((k % 31) * 8) as i32;
        }
        let r = raddoppia(&dimezza(&p), 31, 17);
        assert_eq!((r.w, r.h), (31, 17));
        let max = p.v.iter().zip(&r.v).map(|(a, b)| (a - b).abs()).max().unwrap();
        assert!(max <= 12, "errore massimo {max}");
    }
}
