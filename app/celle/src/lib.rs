//! Immagini a celle esagonali — prima versione (foto), idea del fondatore (docs/design/VIDEO_A_CELLE.md).
//!
//! Regole che si è dato il fondatore il 12/11/2026: aritmetica semplice che qualsiasi dispositivo sa fare, niente
//! formati nuovi e complicati, nessuna dipendenza. Qui c'è solo: somme, medie, differenze, divisioni intere.
//!
//! Come funziona:
//! 1. **Griglia.** Centri su un reticolo triangolare (celle esagonali "a punta in su") con raggio `r` scelto dalla
//!    **complessità** dell'immagine (quanto cambiano i pixel vicini): immagine piatta → celle grandi, immagine ricca
//!    → celle piccole. Ogni pixel appartiene al centro più vicino.
//! 2. **Raffinamento per zona.** Una cella con molto dettaglio dentro si divide in 7 (il suo centro più 6 intorno):
//!    il numero di celle segue la complessità anche zona per zona. Il decodificatore ricostruisce la stessa griglia
//!    da `r` e dall'elenco delle celle raffinate: la geometria non si trasmette.
//! 3. **Strati per cella, colore e dettaglio separati.**
//!    - strato 0 — il **colore** medio della cella (Y, Cb, Cr): poco, arriva subito, basta per vedere l'immagine intera;
//!    - strato 1 — la **pendenza** di luce e colore dentro la cella (sei numeri): sfumature senza "piastrelle";
//!    - strato 2 — il **dettaglio**: l'immagine vera passa per una trasformata a onde intera (`onde`), le bande si
//!      quantizzano e si scrivono con la codifica aritmetica adattiva (`aritmetica`), a pezzi dalla banda più
//!      grossolana alla più fine (`dettaglio`); la media grossolana è prevista dagli strati 0 e 1. Si può anche non
//!      mandare, o mandare a metà: l'immagine resta intera, solo meno fine.
//!
//! Il formato è un elenco di byte semplice (`Codificata`): intestazione + tre strati separati, così ogni strato può
//! viaggiare come blob proprio (ADR-0005) e arrivare quando la banda c'è.

pub mod aritmetica;
pub mod bmp;
pub mod dettaglio;
pub mod onde;
pub mod pacchetto;
pub mod reel;
mod strati;

pub use dettaglio::pezzi_byte as pezzi_dettaglio;

/// Un'immagine RGB a 8 bit per canale, righe dall'alto.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Immagine {
    pub larghezza: u32,
    pub altezza: u32,
    /// `larghezza * altezza * 3` byte, R G B
    pub rgb: Vec<u8>,
}

/// Versione del formato: cambia se cambia il modo di leggere i byte.
pub const VERSIONE: u8 = 3;
const MAGIA: &[u8; 4] = b"M4CL";
/// Scala dei numeri a virgola fissa per le coordinate dei centri (1 pixel = 256).
pub(crate) const S: i64 = 256;
/// √3 · 256, arrotondato: passo orizzontale tra i centri = r · √3.
const RAD3: i64 = 443;
/// Raggio più piccolo e più grande ammessi dal formato (pixel).
/// Pixel al massimo per immagine, in codifica e in decodifica: un'intestazione che dichiara di più viene rifiutata
/// PRIMA di allocare (i pezzi arrivano dalla rete firmati da chiunque: 65535×65535 vorrebbe 17 GB solo di mappa).
pub const PIXEL_MAX: u64 = 2560 * 2560;
pub const RAGGIO_MIN: u16 = 3;
pub const RAGGIO_MAX: u16 = 48;
/// Raggio più piccolo scelto in automatico. Da quando il dettaglio passa per la trasformata (12/11 pomeriggio) le
/// celle servono all'anteprima e alla media grossolana, non al dettaglio: celle piccole costano byte negli strati 0
/// e 1 senza migliorare il risultato finale (foto rumorosa: 170 KB di celle con raggio 13, 3 KB con raggio 30, stessa
/// qualità finale). Misurato il 12/11 su quattro immagini: sotto 30 si perde, tra 30 e 45 cambia poco.
pub const RAGGIO_AUTO_MIN: u16 = 30;
/// La pendenza (strato 1) si misura in 1/32 di livello per pixel.
const SCALA_PENDENZA: i64 = 32;

/// Parametri di codifica scelti da chi chiama (o predefiniti).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parametri {
    /// Passo di quantizzazione del dettaglio della luce (strato 2): più grande = meno byte, meno fine. 0 = niente
    /// strato 2.
    pub passo_dettaglio: u8,
    /// Passo di quantizzazione del dettaglio dei colori (dimezzati). Metà di quello della luce rende meglio sul PSNR
    /// in RGB (gli errori sui colori si amplificano passando a RGB); misurato il 12/11/2026.
    pub passo_croma: u8,
    /// Livelli della trasformata a onde (1–5): più livelli = più pezzi progressivi.
    pub livelli: u8,
    /// Una cella si raffina (7 celle) se lo scarto medio assoluto della sua luce da colore + pendenza supera questa
    /// soglia. 20: si raffinano solo le celle con un bordo netto dentro (con 10 il 10–13 % delle celle, +5 % di byte
    /// in tutto per 0,5 dB in più nell'anteprima; misurato il 12/11). 255 = mai.
    pub soglia_raffinamento: u8,
    /// Forza un raggio invece di ricavarlo dalla complessità (0 = automatico).
    pub raggio: u16,
}

impl Default for Parametri {
    fn default() -> Self {
        Self { passo_dettaglio: 8, passo_croma: 4, livelli: 3, soglia_raffinamento: 20, raggio: 0 }
    }
}

/// Il risultato della codifica: strati separati, ognuno può essere un blob.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Codificata {
    /// intestazione: magia, versione, larghezza, altezza, raggio, passo, passo dei colori, livelli, celle raffinate
    pub testa: Vec<u8>,
    /// strato 0: colore medio per cella (Y, Cb, Cr), compresso (`strati`)
    pub colore: Vec<u8>,
    /// strato 1: pendenza per cella di luce, Cb e Cr (gx, gy per ognuno), compressa (`strati`)
    pub pendenza: Vec<u8>,
    /// strato 2: dettaglio quantizzato e compresso
    pub dettaglio: Vec<u8>,
}

impl Codificata {
    pub fn byte_totali(&self) -> usize {
        self.testa.len() + self.colore.len() + self.pendenza.len() + self.dettaglio.len()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Errore {
    Immagine(&'static str),
    Formato(&'static str),
}

impl std::fmt::Display for Errore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Errore::Immagine(m) => write!(f, "immagine: {m}"),
            Errore::Formato(m) => write!(f, "formato: {m}"),
        }
    }
}
impl std::error::Error for Errore {}

// ─── Colore: RGB ↔ YCbCr in interi (BT.601, come i JPEG) ─────────────────────

#[inline]
pub fn ycbcr(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (r, g, b) = (r as i32, g as i32, b as i32);
    let y = (77 * r + 150 * g + 29 * b + 128) >> 8;
    let cb = ((-43 * r - 85 * g + 128 * b + 128) >> 8) + 128;
    let cr = ((128 * r - 107 * g - 21 * b + 128) >> 8) + 128;
    (y.clamp(0, 255) as u8, cb.clamp(0, 255) as u8, cr.clamp(0, 255) as u8)
}

#[inline]
pub fn rgb(y: i32, cb: u8, cr: u8) -> (u8, u8, u8) {
    let (cb, cr) = (cb as i32 - 128, cr as i32 - 128);
    let r = y + ((359 * cr + 128) >> 8);
    let g = y - ((88 * cb + 183 * cr + 128) >> 8);
    let b = y + ((454 * cb + 128) >> 8);
    (r.clamp(0, 255) as u8, g.clamp(0, 255) as u8, b.clamp(0, 255) as u8)
}

// ─── Complessità e raggio ────────────────────────────────────────────────────

/// Quanto cambiano i pixel vicini: media delle differenze assolute di luce tra un pixel e il suo vicino a destra e
/// in basso, in livelli (0–255). Un cielo liscio sta sotto 3, una foto normale tra 5 e 15, una trama fitta oltre 25.
pub fn complessita(luce: &[u8], larghezza: usize, altezza: usize) -> u32 {
    if larghezza < 2 || altezza < 2 {
        return 0;
    }
    let mut somma: u64 = 0;
    let mut conta: u64 = 0;
    for y in 0..altezza - 1 {
        let riga = &luce[y * larghezza..(y + 1) * larghezza];
        let sotto = &luce[(y + 1) * larghezza..(y + 2) * larghezza];
        for x in 0..larghezza - 1 {
            somma += (riga[x] as i32 - riga[x + 1] as i32).unsigned_abs() as u64;
            somma += (riga[x] as i32 - sotto[x] as i32).unsigned_abs() as u64;
            conta += 2;
        }
    }
    (somma / conta.max(1)) as u32
}

/// Il raggio delle celle dalla complessità: `raggio ≈ 45 / √complessità`, tra `RAGGIO_AUTO_MIN` e `RAGGIO_MAX`:
/// un'immagine piatta ha celle da 45–48 pixel, una foto normale o ricca da 30. Il numero di celle segue poi la
/// complessità zona per zona con il raffinamento.
pub fn raggio_per(complessita: u32) -> u16 {
    // radice intera: il più grande n con n² ≤ complessità·16 (una cifra decimale in più di precisione)
    let c16 = (complessita.max(1) * 16) as u64;
    let mut n = 1u64;
    while (n + 1) * (n + 1) <= c16 {
        n += 1;
    }
    // 45/√c = 45·4/√(16c) = 180/n
    let r = (180 + n / 2) / n;
    (r as u16).clamp(RAGGIO_AUTO_MIN, RAGGIO_MAX)
}

// ─── Griglia ─────────────────────────────────────────────────────────────────

/// La griglia dei centri: reticolo triangolare (righe sfalsate) più i centri delle celle raffinate.
#[derive(Clone, Debug)]
pub struct Griglia {
    pub larghezza: u32,
    pub altezza: u32,
    pub raggio: u16,
    pub(crate) colonne: u32,
    pub(crate) righe: u32,
    /// per ogni cella base: indice della sua prima cella nell'elenco finale (7 se raffinata, 1 se no)
    pub(crate) inizio: Vec<u32>,
    pub(crate) raffinate: Vec<bool>,
    /// centri di tutte le celle finali, in virgola fissa (×256)
    centri: Vec<(i64, i64)>,
}

impl Griglia {
    /// Passo orizzontale (×256) e verticale (×256) tra i centri base.
    fn passi(raggio: u16) -> (i64, i64) {
        (raggio as i64 * RAD3, raggio as i64 * S * 3 / 2)
    }

    /// La griglia base (nessuna cella raffinata).
    pub fn nuova(larghezza: u32, altezza: u32, raggio: u16) -> Self {
        let (hx, vy) = Self::passi(raggio);
        let colonne = ((larghezza as i64 * S + hx - 1) / hx + 1) as u32;
        let righe = ((altezza as i64 * S + vy - 1) / vy + 1) as u32;
        let n = (colonne * righe) as usize;
        let mut g = Griglia { larghezza, altezza, raggio, colonne, righe, inizio: vec![0; n], raffinate: vec![false; n], centri: vec![] };
        g.ricostruisci();
        g
    }

    /// Stessa griglia con queste celle base raffinate (7 centri al posto di 1).
    pub fn con_raffinate(mut self, raffinate: Vec<bool>) -> Self {
        assert_eq!(raffinate.len(), self.raffinate.len());
        self.raffinate = raffinate;
        self.ricostruisci();
        self
    }

    fn centro_base(&self, i: u32, j: u32) -> (i64, i64) {
        let (hx, vy) = Self::passi(self.raggio);
        let sfalso = if j % 2 == 1 { hx / 2 } else { 0 };
        (i as i64 * hx + sfalso, j as i64 * vy)
    }

    fn ricostruisci(&mut self) {
        self.centri.clear();
        // i 6 centri intorno a quello di una cella raffinata: a distanza 0,55·r, verso i vertici dell'esagono
        // (angoli 30°, 90°, …): cos/sin ×256 di 30°: (222, 128); 90°: (0, 256); 150°: (−222, 128)
        let d = self.raggio as i64 * 141; // 0,55 · 256
        let intorno: [(i64, i64); 6] = [(222, 128), (0, 256), (-222, 128), (-222, -128), (0, -256), (222, -128)];
        for j in 0..self.righe {
            for i in 0..self.colonne {
                let k = (j * self.colonne + i) as usize;
                self.inizio[k] = self.centri.len() as u32;
                let (cx, cy) = self.centro_base(i, j);
                self.centri.push((cx, cy));
                if self.raffinate[k] {
                    for (ax, ay) in intorno {
                        self.centri.push((cx + d * ax / 256, cy + d * ay / 256));
                    }
                }
            }
        }
    }

    pub fn celle_base(&self) -> usize {
        (self.colonne * self.righe) as usize
    }
    pub fn celle(&self) -> usize {
        self.centri.len()
    }
    pub fn raffinate(&self) -> &[bool] {
        &self.raffinate
    }

    /// La cella (finale) di un pixel: il centro più vicino tra quelli delle celle base intorno al pixel
    /// (e i loro centri di raffinamento). Solo interi: distanze al quadrato in virgola fissa.
    pub fn cella_di(&self, x: u32, y: u32) -> u32 {
        self.due_celle(x, y).0 .1
    }

    /// Le due celle più vicine a un pixel, con le distanze al quadrato (×256²): la sua e quella oltre il bordo più
    /// vicino. Serve a sfumare tra celle vicine (reel).
    fn due_celle(&self, x: u32, y: u32) -> ((i64, u32), (i64, u32)) {
        let (hx, vy) = Self::passi(self.raggio);
        let px = x as i64 * S + S / 2;
        let py = y as i64 * S + S / 2;
        let j0 = (py / vy) as i64;
        let mut migliore = (i64::MAX, 0u32);
        let mut seconda = (i64::MAX, 0u32);
        for j in (j0 - 1).max(0)..=(j0 + 1).min(self.righe as i64 - 1) {
            let sfalso = if j % 2 == 1 { hx / 2 } else { 0 };
            let i0 = ((px - sfalso) / hx) as i64;
            for i in (i0 - 1).max(0)..=(i0 + 1).min(self.colonne as i64 - 1) {
                let k = (j as u32 * self.colonne + i as u32) as usize;
                let quante = if self.raffinate[k] { 7 } else { 1 };
                let da = self.inizio[k];
                for c in da..da + quante {
                    let (cx, cy) = self.centri[c as usize];
                    let dd = (px - cx) * (px - cx) + (py - cy) * (py - cy);
                    if dd < migliore.0 {
                        seconda = migliore;
                        migliore = (dd, c);
                    } else if dd < seconda.0 {
                        seconda = (dd, c);
                    }
                }
            }
        }
        if seconda.0 == i64::MAX {
            seconda = migliore;
        }
        (migliore, seconda)
    }

    /// Per ogni pixel la cella oltre il bordo più vicino e quanto pesa (in ottavi, 0–4): 4 sul bordo, 0 da metà
    /// raggio in dentro. Con `dd1 ≤ dd2` distanze al quadrato: `peso = (16·dd1 − 8·dd2) / (dd1 + dd2)`, tagliato a
    /// 0–4; solo interi.
    pub fn vicine(&self) -> (Vec<u32>, Vec<u8>) {
        let n = (self.larghezza * self.altezza) as usize;
        let mut vicina = Vec::with_capacity(n);
        let mut peso = Vec::with_capacity(n);
        for y in 0..self.altezza {
            for x in 0..self.larghezza {
                let ((dd1, _), (dd2, c2)) = self.due_celle(x, y);
                let p = if dd1 + dd2 == 0 { 0 } else { ((16 * dd1 - 8 * dd2) / (dd1 + dd2)).clamp(0, 4) };
                vicina.push(c2);
                peso.push(p as u8);
            }
        }
        (vicina, peso)
    }

    /// Mappa e vicine in un giro solo (per il reel, che rifà la geometria a ogni fotogramma).
    pub fn mappa_e_vicine(&self) -> (Vec<u32>, Vec<u32>, Vec<u8>) {
        let n = (self.larghezza * self.altezza) as usize;
        let (mut mappa, mut vicina, mut peso) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
        for y in 0..self.altezza {
            for x in 0..self.larghezza {
                let ((dd1, c1), (dd2, c2)) = self.due_celle(x, y);
                let p = if dd1 + dd2 == 0 { 0 } else { ((16 * dd1 - 8 * dd2) / (dd1 + dd2)).clamp(0, 4) };
                mappa.push(c1);
                vicina.push(c2);
                peso.push(p as u8);
            }
        }
        (mappa, vicina, peso)
    }

    /// Per ogni pixel la sua cella, riga per riga.
    pub fn mappa(&self) -> Vec<u32> {
        let mut m = Vec::with_capacity((self.larghezza * self.altezza) as usize);
        for y in 0..self.altezza {
            for x in 0..self.larghezza {
                m.push(self.cella_di(x, y));
            }
        }
        m
    }

    /// Il centro di una cella finale, in pixel ×256.
    pub fn centro(&self, cella: u32) -> (i64, i64) {
        self.centri[cella as usize]
    }

    /// Il baricentro dei pixel di ogni cella (×256), dalla mappa: è il punto da cui misurare le pendenze. Per una
    /// cella intera coincide con il centro; per una cella tagliata dal bordo dell'immagine no, e misurare dal centro
    /// geometrico storcerebbe il piano (trovato dalla prova del gradiente). Una cella senza pixel tiene il centro.
    pub fn baricentri(&self, mappa: &[u32]) -> Vec<(i64, i64)> {
        let w = self.larghezza as usize;
        let n = self.celle();
        let mut sx = vec![0i64; n];
        let mut sy = vec![0i64; n];
        let mut conta = vec![0i64; n];
        for (k, &c) in mappa.iter().enumerate() {
            sx[c as usize] += (k % w) as i64 * S + S / 2;
            sy[c as usize] += (k / w) as i64 * S + S / 2;
            conta[c as usize] += 1;
        }
        (0..n).map(|c| if conta[c] == 0 { self.centri[c] } else { (sx[c] / conta[c], sy[c] / conta[c]) }).collect()
    }
}

// ─── Byte: numeri a lunghezza variabile e zig-zag ────────────────────────────

pub(crate) fn scrivi_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

pub(crate) fn leggi_varint(b: &[u8], i: &mut usize) -> Result<u64, Errore> {
    let mut v: u64 = 0;
    let mut sh = 0;
    loop {
        let c = *b.get(*i).ok_or(Errore::Formato("numero troncato"))?;
        *i += 1;
        v |= ((c & 0x7f) as u64) << sh;
        if c & 0x80 == 0 {
            return Ok(v);
        }
        sh += 7;
        if sh > 63 {
            return Err(Errore::Formato("numero troppo lungo"));
        }
    }
}

#[inline]
#[allow(dead_code)]
fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}
#[inline]
#[allow(dead_code)]
fn dezigzag(u: u64) -> i64 {
    ((u >> 1) as i64) ^ -((u & 1) as i64)
}

// ─── Codifica ────────────────────────────────────────────────────────────────

/// Scostamento (in pixel interi) di un pixel dal baricentro della sua cella.
#[inline]
fn scostamento(bari: &[(i64, i64)], cella: u32, k: usize, w: usize) -> (i64, i64) {
    let (cx, cy) = bari[cella as usize];
    (((k % w) as i64 * S + S / 2 - cx) / S, ((k / w) as i64 * S + S / 2 - cy) / S)
}

/// Per ogni cella: colore medio (Y, Cb, Cr) e pendenze (gx, gy per Y, Cb, Cr, in 1/32 di livello per pixel) ai
/// minimi quadrati. Due passaggi sui pixel, solo somme e divisioni intere.
fn stima(img: &Immagine, luce: &[u8], bari: &[(i64, i64)], mappa: &[u32]) -> (Vec<(i64, u8, u8)>, Vec<[i8; 6]>) {
    let n = bari.len();
    let w = img.larghezza as usize;
    let mut somma = vec![[0i64; 4]; n]; // n, Y, Cb, Cr
    for (k, px) in img.rgb.chunks_exact(3).enumerate() {
        let (_, cb, cr) = ycbcr(px[0], px[1], px[2]);
        let c = &mut somma[mappa[k] as usize];
        c[0] += 1;
        c[1] += luce[k] as i64;
        c[2] += cb as i64;
        c[3] += cr as i64;
    }
    let media: Vec<(i64, u8, u8)> = somma.iter().map(|c| if c[0] == 0 { (128, 128, 128) } else { ((c[1] + c[0] / 2) / c[0], ((c[2] + c[0] / 2) / c[0]) as u8, ((c[3] + c[0] / 2) / c[0]) as u8) }).collect();
    let mut a = vec![0i64; n];
    let mut b = vec![0i64; n];
    let mut cc = vec![0i64; n];
    let mut pq = vec![[0i64; 6]; n];
    for (k, px) in img.rgb.chunks_exact(3).enumerate() {
        let c = mappa[k] as usize;
        let (dx, dy) = scostamento(bari, c as u32, k, w);
        let (_, cb, cr) = ycbcr(px[0], px[1], px[2]);
        let dev = [luce[k] as i64 - media[c].0, cb as i64 - media[c].1 as i64, cr as i64 - media[c].2 as i64];
        a[c] += dx * dx;
        b[c] += dx * dy;
        cc[c] += dy * dy;
        for (i, d) in dev.iter().enumerate() {
            pq[c][i * 2] += dx * d;
            pq[c][i * 2 + 1] += dy * d;
        }
    }
    let mut pendenze = vec![[0i8; 6]; n];
    for c in 0..n {
        // prodotti di somme su migliaia di pixel: in 128 bit, altrimenti traboccano (trovato dalla prova del gradiente)
        let (aa, bb, c2) = (a[c] as i128, b[c] as i128, cc[c] as i128);
        let det = aa * c2 - bb * bb;
        if det != 0 {
            for i in 0..3 {
                let (pp, qq) = (pq[c][i * 2] as i128, pq[c][i * 2 + 1] as i128);
                let gx = (c2 * pp - bb * qq) * SCALA_PENDENZA as i128 / det;
                let gy = (aa * qq - bb * pp) * SCALA_PENDENZA as i128 / det;
                pendenze[c][i * 2] = gx.clamp(-127, 127) as i8;
                pendenze[c][i * 2 + 1] = gy.clamp(-127, 127) as i8;
            }
        }
    }
    (media, pendenze)
}

fn luce_di(img: &Immagine) -> Vec<u8> {
    img.rgb.chunks_exact(3).map(|p| ycbcr(p[0], p[1], p[2]).0).collect()
}

/// Codifica un'immagine. Restituisce gli strati e la griglia usata (per chi vuole misurare).
pub fn codifica(img: &Immagine, p: Parametri) -> Result<(Codificata, Griglia), Errore> {
    let (w, h) = (img.larghezza as usize, img.altezza as usize);
    if w == 0 || h == 0 || img.rgb.len() != w * h * 3 {
        return Err(Errore::Immagine("dimensioni e byte non tornano"));
    }
    if w > 65535 || h > 65535 || (w as u64) * (h as u64) > PIXEL_MAX {
        return Err(Errore::Immagine("troppo grande"));
    }
    let luce = luce_di(img);
    let raggio = if p.raggio > 0 { p.raggio.clamp(RAGGIO_MIN, RAGGIO_MAX) } else { raggio_per(complessita(&luce, w, h)) };

    // 1. griglia base: colore e pendenza per cella; si raffinano le celle dove la luce NON è spiegata da colore +
    //    pendenza (scarto medio del resto sopra la soglia): un gradiente liscio resta a celle grandi, un bordo no
    let base = Griglia::nuova(img.larghezza, img.altezza, raggio);
    let mappa0 = base.mappa();
    let bari0 = base.baricentri(&mappa0);
    let (media0, pend0) = stima(img, &luce, &bari0, &mappa0);
    let nb = base.celle();
    let mut scarto = vec![0i64; nb];
    let mut conta = vec![0i64; nb];
    for (k, &c) in mappa0.iter().enumerate() {
        let c = c as usize;
        let (dx, dy) = scostamento(&bari0, c as u32, k, w);
        let previsto = media0[c].0 + (pend0[c][0] as i64 * dx + pend0[c][1] as i64 * dy) / SCALA_PENDENZA;
        scarto[c] += (luce[k] as i64 - previsto).abs();
        conta[c] += 1;
    }
    let raffinate: Vec<bool> = (0..nb).map(|c| conta[c] > 0 && scarto[c] / conta[c] > p.soglia_raffinamento as i64).collect();

    // 2. griglia definitiva: colore (strato 0) e pendenze (strato 1) per cella
    let griglia = base.con_raffinate(raffinate);
    let mappa = griglia.mappa();
    let n = griglia.celle();
    let bari = griglia.baricentri(&mappa);
    let (media, pendenze) = stima(img, &luce, &bari, &mappa);
    let mut colore = Vec::with_capacity(n * 3);
    for &(y, cb, cr) in &media {
        colore.extend_from_slice(&[y as u8, cb, cr]);
    }
    let mut pendenza = Vec::with_capacity(n * 6);
    for c in 0..n {
        pendenza.extend(pendenze[c].iter().map(|&v| v as u8));
    }

    // strato 2: dettaglio = bande della trasformata a onde dell'immagine vera, con la media grossolana prevista
    // dall'immagine di base (quella che il decodificatore ricava dagli strati 0 e 1)
    let livelli = p.livelli.clamp(1, dettaglio::LIVELLI_MAX);
    let mut dettaglio = Vec::new();
    if p.passo_dettaglio > 0 {
        let base = piani_base(&mappa, &bari, &colore, &pendenza, w, h);
        let orig = piani_di(img, &luce);
        dettaglio = dettaglio::codifica(&orig, &base, p.passo_dettaglio, p.passo_croma, livelli, &mappa, n, dettaglio::Base::SoloMedia, &mut contesti_nuovi());
    }

    let colore = strati::codifica_colore(&griglia, &colore);
    let pendenza = strati::codifica_pendenza(&griglia, &pendenza);

    // intestazione: magia, versione, w, h, raggio, passi, livelli, celle base raffinate come bit
    let mut testa = Vec::new();
    testa.extend_from_slice(MAGIA);
    testa.push(VERSIONE);
    testa.extend_from_slice(&(w as u16).to_le_bytes());
    testa.extend_from_slice(&(h as u16).to_le_bytes());
    testa.extend_from_slice(&raggio.to_le_bytes());
    testa.push(p.passo_dettaglio);
    testa.push(p.passo_croma);
    testa.push(livelli);
    let bits = griglia.raffinate();
    scrivi_varint(&mut testa, bits.len() as u64);
    let mut byte = 0u8;
    for (i, &r) in bits.iter().enumerate() {
        if r {
            byte |= 1 << (i % 8);
        }
        if i % 8 == 7 {
            testa.push(byte);
            byte = 0;
        }
    }
    if bits.len() % 8 != 0 {
        testa.push(byte);
    }
    Ok((Codificata { testa, colore, pendenza, dettaglio }, griglia))
}

/// Contesti della codifica aritmetica del dettaglio partendo da zero (una foto parte sempre da zero).
pub(crate) fn contesti_nuovi() -> [dettaglio::Contesti; 2] {
    [dettaglio::Contesti::nuovi(), dettaglio::Contesti::nuovi()]
}

/// I piani dell'immagine vera: luce intera, colori dimezzati.
pub(crate) fn piani_di(img: &Immagine, luce: &[u8]) -> dettaglio::Piani {
    let (w, h) = (img.larghezza as usize, img.altezza as usize);
    let mut cb = dettaglio::Piano::nuovo(w, h);
    let mut cr = dettaglio::Piano::nuovo(w, h);
    for (k, px) in img.rgb.chunks_exact(3).enumerate() {
        let (_, b, r) = ycbcr(px[0], px[1], px[2]);
        cb.v[k] = b as i32;
        cr.v[k] = r as i32;
    }
    dettaglio::Piani { y: dettaglio::Piano { w, h, v: luce.iter().map(|&v| v as i32).collect() }, cb: dettaglio::dimezza(&cb), cr: dettaglio::dimezza(&cr) }
}

/// L'immagine di base dagli strati 0 e 1 (colore + pendenza per cella), pixel per pixel: luce intera, colori
/// dimezzati. La usano allo stesso modo chi codifica (per prevedere) e chi decodifica (per ricostruire): deve dare
/// gli stessi numeri da una parte e dall'altra.
fn piani_base(mappa: &[u32], bari: &[(i64, i64)], colore: &[u8], pendenza: &[u8], w: usize, h: usize) -> dettaglio::Piani {
    let mut y = dettaglio::Piano::nuovo(w, h);
    let mut cb = dettaglio::Piano::nuovo(w, h);
    let mut cr = dettaglio::Piano::nuovo(w, h);
    let con_pendenza = !pendenza.is_empty();
    for (k, &cel) in mappa.iter().enumerate() {
        let cel = cel as usize;
        let mut yy = colore[cel * 3] as i64;
        let mut b = colore[cel * 3 + 1] as i64;
        let mut r = colore[cel * 3 + 2] as i64;
        if con_pendenza {
            let (dx, dy) = scostamento(bari, cel as u32, k, w);
            let g = |i: usize| pendenza[cel * 6 + i] as i8 as i64;
            yy += (g(0) * dx + g(1) * dy) / SCALA_PENDENZA;
            b += (g(2) * dx + g(3) * dy) / SCALA_PENDENZA;
            r += (g(4) * dx + g(5) * dy) / SCALA_PENDENZA;
        }
        y.v[k] = yy.clamp(0, 255) as i32;
        cb.v[k] = b.clamp(0, 255) as i32;
        cr.v[k] = r.clamp(0, 255) as i32;
    }
    dettaglio::Piani { y, cb: dettaglio::dimezza(&cb), cr: dettaglio::dimezza(&cr) }
}

/// Quanti strati usare nella decodifica: solo colore, colore e pendenza, o tutto.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strati {
    Colore,
    ColoreEPendenza,
    Tutti,
}

/// Quello che dice l'intestazione di una `Codificata`, griglia compresa.
pub(crate) struct Testa {
    pub w: u32,
    pub h: u32,
    pub passo: u8,
    pub passo_croma: u8,
    pub livelli: u8,
    pub griglia: Griglia,
}

pub(crate) fn leggi_testa(t: &[u8]) -> Result<Testa, Errore> {
    if t.len() < 14 || &t[0..4] != MAGIA {
        return Err(Errore::Formato("intestazione"));
    }
    if t[4] != VERSIONE {
        return Err(Errore::Formato("versione sconosciuta"));
    }
    let w = u16::from_le_bytes([t[5], t[6]]) as u32;
    let h = u16::from_le_bytes([t[7], t[8]]) as u32;
    let raggio = u16::from_le_bytes([t[9], t[10]]);
    let passo = t[11];
    let passo_croma = t[12];
    let livelli = t[13];
    if w == 0 || h == 0 || !(RAGGIO_MIN..=RAGGIO_MAX).contains(&raggio) {
        return Err(Errore::Formato("dimensioni o raggio"));
    }
    if (w as u64) * (h as u64) > PIXEL_MAX {
        return Err(Errore::Formato("troppo grande"));
    }
    if !(1..=dettaglio::LIVELLI_MAX).contains(&livelli) {
        return Err(Errore::Formato("livelli"));
    }
    let mut i = 14;
    let nb = leggi_varint(t, &mut i)? as usize;
    let base = Griglia::nuova(w, h, raggio);
    if nb != base.celle_base() {
        return Err(Errore::Formato("numero di celle"));
    }
    let mut raffinate = vec![false; nb];
    for (k, r) in raffinate.iter_mut().enumerate() {
        let byte = *t.get(i + k / 8).ok_or(Errore::Formato("bit delle celle raffinate troncati"))?;
        *r = byte & (1 << (k % 8)) != 0;
    }
    Ok(Testa { w, h, passo, passo_croma, livelli, griglia: base.con_raffinate(raffinate) })
}

/// Da piani (luce intera, colori dimezzati) all'immagine RGB.
pub(crate) fn piani_in_rgb(p: &dettaglio::Piani) -> Immagine {
    let (w, h) = (p.y.w, p.y.h);
    let (cb, cr) = (dettaglio::raddoppia(&p.cb, w, h), dettaglio::raddoppia(&p.cr, w, h));
    let mut uscita = Vec::with_capacity(w * h * 3);
    for k in 0..w * h {
        let (r, g, b) = rgb(p.y.v[k].clamp(0, 255), cb.v[k].clamp(0, 255) as u8, cr.v[k].clamp(0, 255) as u8);
        uscita.extend_from_slice(&[r, g, b]);
    }
    Immagine { larghezza: w as u32, altezza: h as u32, rgb: uscita }
}

/// Decodifica fino ai piani (luce intera, colori dimezzati): è quello che un reel tiene come riferimento per il
/// fotogramma dopo. Senza dettaglio dà la base degli strati 0 e 1.
pub(crate) fn decodifica_piani(c: &Codificata, strati: Strati) -> Result<(dettaglio::Piani, Griglia), Errore> {
    let t = leggi_testa(&c.testa)?;
    if c.colore.is_empty() {
        return Err(Errore::Formato("strato del colore"));
    }
    let colore = strati::decodifica_colore(&t.griglia, &c.colore);
    let con_pendenza = strati != Strati::Colore && !c.pendenza.is_empty();
    let pendenza: Vec<u8> = if con_pendenza { strati::decodifica_pendenza(&t.griglia, &c.pendenza) } else { vec![] };
    let mappa = t.griglia.mappa();
    let bari = t.griglia.baricentri(&mappa);
    let base = piani_base(&mappa, &bari, &colore, &pendenza, t.w as usize, t.h as usize);
    if strati == Strati::Tutti && con_pendenza && !c.dettaglio.is_empty() && t.passo > 0 {
        let fine = dettaglio::decodifica(&c.dettaglio, &base, t.passo, t.passo_croma, t.livelli, &mappa, t.griglia.celle(), dettaglio::Base::SoloMedia, &mut contesti_nuovi());
        return Ok((fine, t.griglia));
    }
    Ok((base, t.griglia))
}

/// Decodifica. Gli strati mancanti (vuoti) si saltano: l'immagine esce intera, meno fine.
pub fn decodifica(c: &Codificata, strati: Strati) -> Result<Immagine, Errore> {
    let Testa { w, h, passo, passo_croma, livelli, griglia } = leggi_testa(&c.testa)?;
    if c.colore.is_empty() {
        return Err(Errore::Formato("strato del colore"));
    }
    let colore = strati::decodifica_colore(&griglia, &c.colore);
    let con_pendenza = strati != Strati::Colore && !c.pendenza.is_empty();
    let pendenza: Vec<u8> = if con_pendenza { strati::decodifica_pendenza(&griglia, &c.pendenza) } else { vec![] };
    let mappa = griglia.mappa();
    let bari = griglia.baricentri(&mappa);
    let (w, h) = (w as usize, h as usize);
    let mut uscita = Vec::with_capacity(w * h * 3);
    // il dettaglio si applica solo sopra colore + pendenza: è stato previsto così da chi ha codificato
    if strati == Strati::Tutti && con_pendenza && !c.dettaglio.is_empty() && passo > 0 {
        let base = piani_base(&mappa, &bari, &colore, &pendenza, w, h);
        let fine = dettaglio::decodifica(&c.dettaglio, &base, passo, passo_croma, livelli, &mappa, griglia.celle(), dettaglio::Base::SoloMedia, &mut contesti_nuovi());
        return Ok(piani_in_rgb(&fine));
    }

    // senza dettaglio: colore e pendenza per cella, pixel per pixel, a piena risoluzione anche nei colori
    for (k, &cel) in mappa.iter().enumerate() {
        let cel = cel as usize;
        let mut y = colore[cel * 3] as i64;
        let mut cb = colore[cel * 3 + 1] as i64;
        let mut cr = colore[cel * 3 + 2] as i64;
        if con_pendenza {
            let (dx, dy) = scostamento(&bari, cel as u32, k, w);
            let g = |i: usize| pendenza[cel * 6 + i] as i8 as i64;
            y += (g(0) * dx + g(1) * dy) / SCALA_PENDENZA;
            cb += (g(2) * dx + g(3) * dy) / SCALA_PENDENZA;
            cr += (g(4) * dx + g(5) * dy) / SCALA_PENDENZA;
        }
        let (r, g, b) = rgb(y.clamp(0, 255) as i32, cb.clamp(0, 255) as u8, cr.clamp(0, 255) as u8);
        uscita.extend_from_slice(&[r, g, b]);
    }
    Ok(Immagine { larghezza: w as u32, altezza: h as u32, rgb: uscita })
}

/// PSNR (dB ×100) della sola luce: dice quanto è fine il dettaglio senza il peso dei colori.
pub fn psnr_luce_x100(a: &Immagine, b: &Immagine) -> Option<u32> {
    if a.rgb.len() != b.rgb.len() || a.rgb.is_empty() {
        return None;
    }
    let (la, lb) = (luce_di(a), luce_di(b));
    let se: u64 = la.iter().zip(&lb).map(|(x, y)| ((*x as i64 - *y as i64).pow(2)) as u64).sum();
    if se == 0 {
        return Some(9999);
    }
    let num = 65025u64 * la.len() as u64;
    Some((10 * (log10_x100(num) as i64 - log10_x100(se) as i64)).max(0) as u32)
}

/// Rapporto segnale/rumore di picco (dB ×100) tra due immagini: più alto = più simili; oltre 3500 (35 dB) l'occhio
/// fatica a vedere differenze, sotto 2800 si vedono. Aritmetica intera, logaritmo per tabella.
pub fn psnr_x100(a: &Immagine, b: &Immagine) -> Option<u32> {
    if a.rgb.len() != b.rgb.len() || a.rgb.is_empty() {
        return None;
    }
    let mut se: u64 = 0;
    for (x, y) in a.rgb.iter().zip(&b.rgb) {
        let d = *x as i64 - *y as i64;
        se += (d * d) as u64;
    }
    if se == 0 {
        return Some(9999);
    }
    // PSNR = 10·log10(255²·n / se) = 10·(log10(255²·n) − log10(se)); log10 con 2 decimali per interi
    let num = 65025u64 * a.rgb.len() as u64;
    Some((10 * (log10_x100(num) as i64 - log10_x100(se) as i64)).max(0) as u32)
}

/// log10(v)·100 per interi, con la parte frazionaria stimata per bisezione sulle potenze: basta per confrontare.
fn log10_x100(v: u64) -> u32 {
    if v == 0 {
        return 0;
    }
    let mut int = 0u32;
    let mut r = v;
    while r >= 10 {
        r /= 10;
        int += 1;
    }
    // frazione: v / 10^int in [1,10) → cerca f in 0..100 con 10^(f/100) ≤ mantissa
    let base = 10u64.pow(int);
    let mut lo = 0u32;
    let mut hi = 100u32;
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        // 10^(mid/100) confrontato con v/base: (v/base)^100 ≥ 10^mid ⇔ … troppo grande; si usa una tabella di 10^(k/100)·2^20
        if pow10_frac_x2p20(mid) as u128 * base as u128 <= (v as u128) << 20 {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    int * 100 + lo
}

/// 10^(k/100) · 2^20 per k = 0..=100, calcolato con la radice: ripetere 100 volte la radice centesima non è
/// "aritmetica semplice", quindi si usa 2^(k·log2(10)/100) via una tabella di 2^(j/64).
fn pow10_frac_x2p20(k: u32) -> u64 {
    // k/100 · log2(10) = k · 3,321928 / 100 → in 64esimi: k · 212,6 / 100
    let j = (k as u64 * 2126 + 500) / 1000; // 64esimi di bit
    let interi = j / 64;
    let frac = j % 64;
    // 2^(frac/64) · 2^20, tabella a passi di 1/64 (valori arrotondati)
    const T: [u32; 64] = [
        1048576, 1059985, 1071519, 1083179, 1094966, 1106880, 1118925, 1131101, 1143409, 1155851, 1168429, 1181143, 1193996, 1206989, 1220124, 1233402, 1246824, 1260391, 1274107, 1287971, 1301986,
        1316154, 1330476, 1344953, 1359588, 1374382, 1389337, 1404454, 1419736, 1435185, 1450801, 1466587, 1482545, 1498676, 1514983, 1531468, 1548132, 1564978, 1582006, 1599221, 1616622, 1634213,
        1651995, 1669971, 1688142, 1706511, 1725080, 1743852, 1762827, 1782009, 1801399, 1821001, 1840816, 1860847, 1881095, 1901564, 1922255, 1943172, 1964316, 1985690, 2007297, 2029139, 2051219,
        2073539,
    ];
    (T[frac as usize] as u64) << interi
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradiente(w: u32, h: u32) -> Immagine {
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                rgb.extend_from_slice(&[(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, 128]);
            }
        }
        Immagine { larghezza: w, altezza: h, rgb }
    }

    fn scacchi(w: u32, h: u32, lato: u32) -> Immagine {
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                let v = if ((x / lato) + (y / lato)) % 2 == 0 { 30 } else { 220 };
                rgb.extend_from_slice(&[v, v, v]);
            }
        }
        Immagine { larghezza: w, altezza: h, rgb }
    }

    #[test]
    fn colore_avanti_e_indietro() {
        for (r, g, b) in [(0, 0, 0), (255, 255, 255), (255, 0, 0), (0, 255, 0), (0, 0, 255), (12, 200, 99)] {
            let (y, cb, cr) = ycbcr(r, g, b);
            let (r2, g2, b2) = rgb(y as i32, cb, cr);
            assert!((r as i32 - r2 as i32).abs() <= 2 && (g as i32 - g2 as i32).abs() <= 2 && (b as i32 - b2 as i32).abs() <= 2, "{r},{g},{b} → {r2},{g2},{b2}");
        }
    }

    #[test]
    fn il_raggio_segue_la_complessita() {
        assert!(raggio_per(1) > raggio_per(8), "piatto → celle più grandi");
        assert!(raggio_per(8) >= raggio_per(40), "ricco → celle non più grandi");
        assert_eq!(raggio_per(0), RAGGIO_MAX.min(raggio_per(1)));
        assert_eq!(raggio_per(10_000), RAGGIO_AUTO_MIN);
        assert_eq!(raggio_per(8), RAGGIO_AUTO_MIN, "una foto normale sta al minimo automatico");
        assert!((40..=48).contains(&raggio_per(1)), "{}", raggio_per(1));
    }

    #[test]
    fn ogni_pixel_ha_una_cella_e_le_celle_sono_compatte() {
        let g = Griglia::nuova(100, 70, 10);
        let m = g.mappa();
        assert_eq!(m.len(), 7000);
        assert!(m.iter().all(|&c| (c as usize) < g.celle()));
        // una cella esagonale di raggio 10 ha area ≈ 260 pixel: nessuna cella interna ne ha molti di più o di meno
        let mut conta = vec![0u32; g.celle()];
        for &c in &m {
            conta[c as usize] += 1;
        }
        let interne: Vec<u32> = conta.iter().copied().filter(|&n| n > 200).collect();
        assert!(!interne.is_empty() && interne.iter().all(|&n| n <= 300), "{interne:?}");
        // raffinando una cella, i suoi pixel si dividono in 7 e il totale non cambia
        let mut raff = vec![false; g.celle_base()];
        raff[g.celle_base() / 2] = true;
        let g2 = Griglia::nuova(100, 70, 10).con_raffinate(raff);
        assert_eq!(g2.celle(), g.celle() + 6);
        assert_eq!(g2.mappa().len(), 7000);
    }

    #[test]
    fn numeri_a_lunghezza_variabile_e_zigzag() {
        for v in [0i64, 1, -1, 63, -64, 1000, -1000, i32::MAX as i64, i32::MIN as i64] {
            let mut b = vec![];
            scrivi_varint(&mut b, zigzag(v));
            let mut i = 0;
            assert_eq!(dezigzag(leggi_varint(&b, &mut i).unwrap()), v);
            assert_eq!(i, b.len());
        }
        assert!(leggi_varint(&[0x80], &mut 0).is_err(), "troncato");
    }

    #[test]
    fn un_gradiente_si_ricostruisce_bene_gia_con_colore_e_pendenza() {
        let img = gradiente(160, 120);
        let (c, g) = codifica(&img, Parametri::default()).unwrap();
        assert!(g.celle() >= 12 && g.raffinate().iter().all(|&r| !r), "un gradiente liscio non raffina niente: {} celle", g.celle());
        let solo_colore = decodifica(&c, Strati::Colore).unwrap();
        let con_pendenza = decodifica(&c, Strati::ColoreEPendenza).unwrap();
        let tutto = decodifica(&c, Strati::Tutti).unwrap();
        let (p0, p1, p2) = (psnr_x100(&img, &solo_colore).unwrap(), psnr_x100(&img, &con_pendenza).unwrap(), psnr_x100(&img, &tutto).unwrap());
        assert!(p0 < p1, "la pendenza deve migliorare il colore: {p0} {p1}");
        assert!(p1 > 3000, "un gradiente con la pendenza deve superare 30 dB: {p1}");
        // qui il dettaglio ha poco da aggiungere: resta sopra i 40 dB (i colori dimezzati costano qualche decimo)
        assert!(p2 > 4000, "gradiente con il dettaglio: {p2}");
        assert!(c.dettaglio.len() < 400, "su un gradiente il dettaglio è quasi vuoto: {} byte", c.dettaglio.len());
    }

    #[test]
    fn una_scacchiera_raffina_le_celle_sui_bordi_e_il_dettaglio_la_ricostruisce() {
        let img = scacchi(128, 128, 16);
        let (c, g) = codifica(&img, Parametri { passo_dettaglio: 4, ..Parametri::default() }).unwrap();
        let raffinate = g.raffinate().iter().filter(|&&r| r).count();
        assert!(raffinate > 0, "nessuna cella raffinata su una scacchiera");
        let tutto = decodifica(&c, Strati::Tutti).unwrap();
        let p = psnr_x100(&img, &tutto).unwrap();
        assert!(p > 3000, "scacchiera con il dettaglio: {p}");
        // senza dettaglio l'immagine è comunque intera, con gli stessi byte di colore
        let senza = Codificata { dettaglio: vec![], ..c.clone() };
        let d = decodifica(&senza, Strati::Tutti).unwrap();
        assert_eq!(d.rgb.len(), img.rgb.len());
        assert!(psnr_x100(&img, &d).unwrap() < p);
    }

    #[test]
    fn byte_rotti_non_fanno_cadere_niente() {
        let img = gradiente(64, 48);
        let (c, _) = codifica(&img, Parametri::default()).unwrap();
        let mut t = c.clone();
        t.testa[0] = b'X';
        assert_eq!(decodifica(&t, Strati::Tutti).unwrap_err(), Errore::Formato("intestazione"));
        // intestazione ben formata ma con misure enormi: rifiutata prima di allocare la mappa dei pixel
        let mut enorme = MAGIA.to_vec();
        enorme.extend_from_slice(&[VERSIONE, 0xff, 0xff, 0xff, 0xff, 48, 0, 8, 8, 2]);
        enorme.extend_from_slice(&[0; 64]);
        let t = Codificata { testa: enorme, colore: vec![1, 2, 3], pendenza: vec![], dettaglio: vec![] };
        assert_eq!(decodifica(&t, Strati::Tutti).unwrap_err(), Errore::Formato("troppo grande"));
        assert!(codifica(&Immagine { larghezza: 4000, altezza: 2000, rgb: vec![0; 4000 * 2000 * 3] }, Parametri::default()).is_err());
        let mut v = c.clone();
        v.testa[4] = 9;
        assert!(decodifica(&v, Strati::Tutti).is_err());
        let mut col = c.clone();
        col.colore.clear();
        assert_eq!(decodifica(&col, Strati::Tutti).unwrap_err(), Errore::Formato("strato del colore"));
        let mut col2 = c.clone();
        col2.colore.truncate(col2.colore.len() / 2);
        assert_eq!(decodifica(&col2, Strati::Tutti).unwrap().rgb.len(), img.rgb.len(), "colore a metà: immagine intera, colori sbagliati, nessun errore");
        let mut det = c.clone();
        det.dettaglio = vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f];
        let _ = decodifica(&det, Strati::Tutti); // qualunque esito, ma senza panico
        let mut tronco = c.clone();
        tronco.dettaglio.truncate(tronco.dettaglio.len() / 2);
        assert!(decodifica(&tronco, Strati::Tutti).is_ok(), "un dettaglio a metà dà un'immagine a metà, non un errore");
        assert_eq!(decodifica(&c, Strati::Tutti).unwrap().rgb.len(), img.rgb.len());
    }

    #[test]
    fn psnr_e_log_sono_ragionevoli() {
        assert_eq!(log10_x100(1000), 300);
        assert!((199..=201).contains(&log10_x100(100)));
        assert!((69..=71).contains(&log10_x100(5)), "{}", log10_x100(5));
        let a = gradiente(32, 32);
        assert_eq!(psnr_x100(&a, &a), Some(9999));
        let mut b = a.clone();
        for v in b.rgb.iter_mut() {
            *v = v.saturating_add(10);
        }
        let p = psnr_x100(&a, &b).unwrap();
        assert!((2800..=2830).contains(&p), "errore costante di 10 → 28,1 dB, avuto {p}");
    }
}
