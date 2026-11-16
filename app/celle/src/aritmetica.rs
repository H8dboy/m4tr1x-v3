//! Codifica aritmetica binaria adattiva (schema "range coder" di LZMA): ogni bit ha una probabilità su 15 bit che si
//! aggiorna da sola dopo ogni bit; un bit molto probabile costa una frazione di bit. Solo moltiplicazioni, spostamenti
//! e somme su interi: niente tabelle, niente virgola mobile.

const BIT_PROB: u32 = 15;
/// Due finestre di adattamento: una corta (~2^4 bit) che segue le zone, una lunga (~2^6) che tiene la statistica.
/// Misurato il 13/11/2026: con 11 bit di precisione la finestra lunga non poteva superare il 94 % e peggiorava
/// tutto; con 15 bit le coppie 3/6, 4/6, 4/7 sono alla pari (−10 % di byte rispetto a una finestra sola da 2^5).
const ADATTAMENTO_CORTO: u32 = 4;
const ADATTAMENTO_LUNGO: u32 = 6;

/// Una probabilità adattiva: `p` = probabilità che il bit sia 0 (su 2048), `n` = quanti bit ha già visto. I primi
/// bit la muovono molto (finestra 2, 4, 8…), poi si assesta su una finestra di 2^ADATTAMENTO_MAX: così un contesto
/// nuovo impara in pochi bit invece di sprecarne decine a metà strada (misurato: −15 % di byte sul render).
#[derive(Clone, Copy, Debug)]
pub struct Prob {
    /// stima con la finestra corta
    pc: u16,
    /// stima con la finestra lunga
    pl: u16,
    n: u8,
}

impl Prob {
    pub const NUOVA: Prob = Prob { pc: 1 << (BIT_PROB - 1), pl: 1 << (BIT_PROB - 1), n: 0 };

    /// La probabilità usata: la media delle due stime.
    #[inline]
    fn p(&self) -> u32 {
        ((self.pc as u32 + self.pl as u32 + 1) >> 1).max(1)
    }

    #[inline]
    fn aggiorna(&mut self, bit: bool) {
        let riscaldamento = u16::BITS - (self.n as u16 + 1).leading_zeros();
        let sc = riscaldamento.clamp(1, ADATTAMENTO_CORTO);
        let sl = riscaldamento.clamp(1, ADATTAMENTO_LUNGO);
        if bit {
            self.pc -= self.pc >> sc;
            self.pl -= self.pl >> sl;
        } else {
            self.pc += ((1u16 << BIT_PROB) - self.pc) >> sc;
            self.pl += ((1u16 << BIT_PROB) - self.pl) >> sl;
        }
        self.n = self.n.saturating_add(1);
    }
}
const SOGLIA: u32 = 1 << 24;

/// Scrive bit, produce byte.
pub struct Scrittore {
    low: u64,
    range: u32,
    cache: u8,
    in_attesa: u64,
    out: Vec<u8>,
}

impl Default for Scrittore {
    fn default() -> Self {
        Self::nuovo()
    }
}

impl Scrittore {
    pub fn nuovo() -> Self {
        Self { low: 0, range: 0xFFFF_FFFF, cache: 0, in_attesa: 1, out: Vec::new() }
    }

    /// Un bit con la sua probabilità adattiva (`p` = probabilità che il bit sia 0, su 2048).
    #[inline]
    pub fn bit(&mut self, p: &mut Prob, bit: bool) {
        let bound = (self.range >> BIT_PROB) * p.p();
        if !bit {
            self.range = bound;
        } else {
            self.low += bound as u64;
            self.range -= bound;
        }
        p.aggiorna(bit);
        while self.range < SOGLIA {
            self.range <<= 8;
            self.sposta();
        }
    }

    /// Un bit "a metà", senza adattamento (per i bit che sono davvero casuali).
    #[inline]
    pub fn bit_diretto(&mut self, bit: bool) {
        self.range >>= 1;
        if bit {
            self.low += self.range as u64;
        }
        while self.range < SOGLIA {
            self.range <<= 8;
            self.sposta();
        }
    }

    fn sposta(&mut self) {
        if (self.low as u32) < 0xFF00_0000 || (self.low >> 32) != 0 {
            let riporto = (self.low >> 32) as u8;
            let mut b = self.cache;
            loop {
                self.out.push(b.wrapping_add(riporto));
                b = 0xFF;
                self.in_attesa -= 1;
                if self.in_attesa == 0 {
                    break;
                }
            }
            self.cache = ((self.low >> 24) & 0xFF) as u8;
        }
        self.in_attesa += 1;
        self.low = (self.low & 0x00FF_FFFF) << 8;
    }

    /// Chiude e restituisce i byte.
    pub fn chiudi(mut self) -> Vec<u8> {
        for _ in 0..5 {
            self.sposta();
        }
        self.out
    }
}

/// Legge bit dai byte. Oltre la fine legge zeri: chi chiama sa quanti bit aspettarsi.
pub struct Lettore<'a> {
    b: &'a [u8],
    i: usize,
    range: u32,
    code: u32,
}

impl<'a> Lettore<'a> {
    pub fn nuovo(b: &'a [u8]) -> Self {
        let mut l = Self { b, i: 0, range: 0xFFFF_FFFF, code: 0 };
        for _ in 0..5 {
            l.code = (l.code << 8) | l.prossimo() as u32;
        }
        l
    }

    #[inline]
    fn prossimo(&mut self) -> u8 {
        let v = self.b.get(self.i).copied().unwrap_or(0);
        self.i += 1;
        v
    }

    #[inline]
    pub fn bit(&mut self, p: &mut Prob) -> bool {
        let bound = (self.range >> BIT_PROB) * p.p();
        let bit = if self.code < bound {
            self.range = bound;
            false
        } else {
            self.code -= bound;
            self.range -= bound;
            true
        };
        p.aggiorna(bit);
        while self.range < SOGLIA {
            self.range <<= 8;
            self.code = (self.code << 8) | self.prossimo() as u32;
        }
        bit
    }

    #[inline]
    pub fn bit_diretto(&mut self) -> bool {
        self.range >>= 1;
        let bit = if self.code >= self.range {
            self.code -= self.range;
            true
        } else {
            false
        };
        while self.range < SOGLIA {
            self.range <<= 8;
            self.code = (self.code << 8) | self.prossimo() as u32;
        }
        bit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generatore pseudo-casuale piccolo (xorshift), solo per le prove.
    fn caso(stato: &mut u64) -> u64 {
        *stato ^= *stato << 13;
        *stato ^= *stato >> 7;
        *stato ^= *stato << 17;
        *stato
    }

    #[test]
    fn bit_avanti_e_indietro_con_probabilita_diverse() {
        let mut stato = 0x9E37_79B9_7F4A_7C15u64;
        for giro in 0..20 {
            let n = 1 + (caso(&mut stato) % 5000) as usize;
            // bit per lo più zero in alcuni contesti, casuali in altri
            let mut bits = Vec::with_capacity(n);
            let mut ctx = Vec::with_capacity(n);
            for _ in 0..n {
                let c = (caso(&mut stato) % 4) as usize;
                let soglia = [1u64, 20, 60, 128][c];
                bits.push(caso(&mut stato) % 128 < soglia);
                ctx.push(c);
            }
            let mut p = [Prob::NUOVA; 4];
            let mut s = Scrittore::nuovo();
            for (b, &c) in bits.iter().zip(&ctx) {
                if c == 3 && giro % 2 == 0 {
                    s.bit_diretto(*b);
                } else {
                    s.bit(&mut p[c], *b);
                }
            }
            let out = s.chiudi();
            let mut p = [Prob::NUOVA; 4];
            let mut l = Lettore::nuovo(&out);
            for (k, (b, &c)) in bits.iter().zip(&ctx).enumerate() {
                let letto = if c == 3 && giro % 2 == 0 { l.bit_diretto() } else { l.bit(&mut p[c]) };
                assert_eq!(letto, *b, "giro {giro} bit {k}");
            }
            // tre contesti su quattro sono sbilanciati → molto meno di un byte ogni 8 bit
            assert!(out.len() * 8 < n + 64, "{n} bit → {} byte", out.len());
        }
    }

    #[test]
    fn byte_mancanti_non_fanno_cadere_niente() {
        let mut p = Prob::NUOVA;
        let mut l = Lettore::nuovo(&[]);
        for _ in 0..100 {
            let _ = l.bit(&mut p);
            let _ = l.bit_diretto();
        }
    }
}
