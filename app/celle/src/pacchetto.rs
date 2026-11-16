//! Il **pacchetto**: come una foto o un reel a celle viaggia in rete, a pezzi (ADR-0005). Ogni pezzo è un blob
//! con il suo hash; chi guarda scarica i pezzi in ordine e mostra l'immagine intera dal PRIMO (l'anteprima: celle,
//! colore e pendenza, pochi KB), poi la affina pezzo dopo pezzo, finché la banda lo permette. Non c'è niente da
//! ricodificare in mezzo: i pezzi sono gli stessi byte scritti da chi carica.
//!
//! **Foto** (`pezzi_foto` / `apri_foto`): il pezzo 0 è `M4CF` + intestazione + strato 0 + strato 1 (ognuno con la
//! lunghezza davanti); i pezzi dopo sono lo strato 2 diviso per livello di finezza (le medie grossolane stanno con
//! il livello più grossolano). I pezzi dello strato 2 sono un prefisso dello strato intero: se ne mancano in coda,
//! la decodifica è quella dei pezzi che ci sono.
//!
//! **Reel** (`pezzi_reel` / `apri_reel`): un pezzo per gruppo di fotogrammi da una chiave alla successiva (in ordine
//! di decodifica), ognuno un reel completo (`M4RL`) che si legge da solo: i fotogrammi del gruppo escono appena il
//! pezzo arriva. Dal 16/11 l'encoder garantisce che un gruppo dipende solo dalla sua chiave.

use crate::reel::{decodifica_reel, Fotogramma, Reel};
use crate::{decodifica, dettaglio, leggi_varint, pezzi_dettaglio, scrivi_varint, Codificata, Errore, Immagine, Strati};

const MAGIA_FOTO: &[u8; 4] = b"M4CF";
const MAGIA_REEL: &[u8; 4] = b"M4RL";

/// Che cosa c'è in un pezzo 0, dai primi byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tipo {
    Foto,
    Reel,
}

/// Il tipo di un pacchetto dai primi byte del primo pezzo (None: non è un pacchetto a celle).
pub fn tipo(primo_pezzo: &[u8]) -> Option<Tipo> {
    match primo_pezzo.get(0..4)? {
        m if m == MAGIA_FOTO => Some(Tipo::Foto),
        m if m == MAGIA_REEL => Some(Tipo::Reel),
        _ => None,
    }
}

fn con_lunghezza(out: &mut Vec<u8>, p: &[u8]) {
    scrivi_varint(out, p.len() as u64);
    out.extend_from_slice(p);
}

fn pezzo_con_lunghezza<'a>(b: &'a [u8], i: &mut usize) -> Result<&'a [u8], Errore> {
    let l = leggi_varint(b, i)? as usize;
    let fine = i.checked_add(l).filter(|&f| f <= b.len()).ok_or(Errore::Formato("pezzo troncato"))?;
    let p = &b[*i..fine];
    *i = fine;
    Ok(p)
}

/// I pezzi di una foto codificata, in ordine: anteprima, poi un pezzo per livello di finezza dello strato 2 (dal
/// più grossolano). Una foto senza strato 2 è un pezzo solo.
pub fn pezzi_foto(c: &Codificata) -> Vec<Vec<u8>> {
    let mut primo = Vec::with_capacity(c.testa.len() + c.colore.len() + c.pendenza.len() + 16);
    primo.extend_from_slice(MAGIA_FOTO);
    con_lunghezza(&mut primo, &c.testa);
    con_lunghezza(&mut primo, &c.colore);
    con_lunghezza(&mut primo, &c.pendenza);
    let mut pezzi = vec![primo];
    // i sotto-pezzi dello strato 2: le medie, poi GRUPPI per livello; un blob per livello, le medie con il primo
    let fini = pezzi_dettaglio(&c.dettaglio);
    let mut da = 0usize;
    let mut blob: Vec<u8> = Vec::new();
    for (s, &fine) in fini.iter().enumerate() {
        blob.extend_from_slice(&c.dettaglio[da..fine]);
        da = fine;
        let ultimo_del_livello = s > 0 && (s - 1) % dettaglio::GRUPPI == dettaglio::GRUPPI - 1;
        if ultimo_del_livello {
            pezzi.push(std::mem::take(&mut blob));
        }
    }
    if !blob.is_empty() {
        pezzi.push(blob);
    }
    pezzi
}

/// La foto dai pezzi che ci sono (almeno il primo): i pezzi dello strato 2 mancanti in coda la lasciano meno fine.
pub fn foto_da_pezzi(pezzi: &[&[u8]]) -> Result<Codificata, Errore> {
    let primo = *pezzi.first().ok_or(Errore::Formato("nessun pezzo"))?;
    if tipo(primo) != Some(Tipo::Foto) {
        return Err(Errore::Formato("non è una foto a celle"));
    }
    let mut i = 4;
    let testa = pezzo_con_lunghezza(primo, &mut i)?.to_vec();
    let colore = pezzo_con_lunghezza(primo, &mut i)?.to_vec();
    let pendenza = pezzo_con_lunghezza(primo, &mut i)?.to_vec();
    if i != primo.len() {
        return Err(Errore::Formato("byte in più nel primo pezzo"));
    }
    let dettaglio: Vec<u8> = pezzi[1..].iter().flat_map(|p| p.iter().copied()).collect();
    Ok(Codificata { testa, colore, pendenza, dettaglio })
}

/// Decodifica la foto dai pezzi che ci sono.
pub fn apri_foto(pezzi: &[&[u8]]) -> Result<Immagine, Errore> {
    decodifica(&foto_da_pezzi(pezzi)?, Strati::Tutti)
}

/// I pezzi di un reel: uno per gruppo da una chiave alla successiva, ognuno un reel completo che si legge da solo.
pub fn pezzi_reel(r: &Reel) -> Vec<Vec<u8>> {
    let mut pezzi = Vec::new();
    let mut gruppo: Vec<Fotogramma> = Vec::new();
    for f in &r.fotogrammi {
        if matches!(f, Fotogramma::Chiave(_)) && !gruppo.is_empty() {
            pezzi.push(Reel { fotogrammi: std::mem::take(&mut gruppo), fps: r.fps }.in_byte());
        }
        gruppo.push(f.clone());
    }
    if !gruppo.is_empty() {
        pezzi.push(Reel { fotogrammi: gruppo, fps: r.fps }.in_byte());
    }
    pezzi
}

/// I fotogrammi di UN pezzo di reel, in ordine di visualizzazione, e i fotogrammi al secondo.
pub fn apri_reel(pezzo: &[u8]) -> Result<(Vec<Immagine>, u8), Errore> {
    let r = Reel::da_byte(pezzo)?;
    if !matches!(r.fotogrammi.first(), Some(Fotogramma::Chiave(_))) {
        return Err(Errore::Formato("un pezzo di reel parte da una chiave"));
    }
    Ok((decodifica_reel(&r)?, r.fps))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reel::{codifica_reel, ParametriReel};
    use crate::{codifica, psnr_x100, Parametri};

    fn foto(w: u32, h: u32) -> Immagine {
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                let bordo = if (x / 23 + y / 17) % 2 == 0 { 40 } else { 0 };
                rgb.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8 ^ bordo, ((x + y) % 97) as u8]);
            }
        }
        Immagine { larghezza: w, altezza: h, rgb }
    }

    #[test]
    fn la_foto_viaggia_a_pezzi_e_ogni_pezzo_in_piu_la_affina() {
        let img = foto(200, 150);
        let (c, _) = codifica(&img, Parametri { raggio: 12, ..Parametri::default() }).unwrap();
        let pezzi = pezzi_foto(&c);
        assert_eq!(pezzi.len(), 1 + Parametri::default().livelli as usize, "anteprima + un pezzo per livello");
        assert_eq!(tipo(&pezzi[0]), Some(Tipo::Foto));
        let in_piu = pezzi.iter().map(|p| p.len()).sum::<usize>() - c.byte_totali();
        assert!((7..=13).contains(&in_piu), "solo la magia e tre lunghezze in più: {in_piu}");
        let viste: Vec<&[u8]> = pezzi.iter().map(|p| p.as_slice()).collect();
        assert_eq!(foto_da_pezzi(&viste).unwrap(), c, "dai pezzi si ricompone esattamente la codificata");
        let mut prima = 0u32;
        for k in 1..=pezzi.len() {
            let d = apri_foto(&viste[..k]).unwrap();
            let p = psnr_x100(&img, &d).unwrap();
            assert!(p >= prima, "il pezzo {k} peggiora: {p} < {prima}");
            prima = p;
        }
        assert_eq!(apri_foto(&viste).unwrap(), decodifica(&c, Strati::Tutti).unwrap());
        // senza strato 2: un pezzo solo
        let (solo, _) = codifica(&img, Parametri { passo_dettaglio: 0, raggio: 12, ..Parametri::default() }).unwrap();
        assert_eq!(pezzi_foto(&solo).len(), 1);
        // pezzi rotti: errore o immagine, mai panico
        assert!(apri_foto(&[]).is_err());
        assert!(apri_foto(&[b"M4RL"]).is_err());
        assert!(apri_foto(&[&pezzi[0][..pezzi[0].len() / 2]]).is_err());
        let mut gonfio = pezzi[0].clone();
        gonfio.push(0);
        assert!(apri_foto(&[&gonfio]).is_err());
        let _ = apri_foto(&[&pezzi[0], &[7u8; 50]]);
    }

    #[test]
    fn il_reel_viaggia_a_gruppi_che_si_leggono_da_soli() {
        let f: Vec<Immagine> = (0..13).map(|i| crate::reel::prova::scena(128, 96, 2 * i, i, 0)).collect();
        let p = ParametriReel { foto: Parametri { raggio: 12, ..Parametri::default() }, chiave_ogni: 8, cerca: 8, soglia_chiave: 0, bidirezionali: 3, celle_mobili: true, fps: 12 };
        let r = codifica_reel(&f, p).unwrap();
        let tutti = decodifica_reel(&r).unwrap();
        assert_eq!(tutti.len(), 13);
        let pezzi = pezzi_reel(&r);
        assert_eq!(pezzi.len(), 2, "chiavi a 0 e 8");
        assert_eq!(tipo(&pezzi[0]), Some(Tipo::Reel));
        let mut visti = Vec::new();
        for pz in &pezzi {
            let (fotogrammi, fps) = apri_reel(pz).unwrap();
            assert_eq!(fps, 12);
            visti.extend(fotogrammi);
        }
        assert_eq!(visti, tutti, "gruppo per gruppo si vede lo stesso reel");
        assert!(r.fotogrammi.iter().any(|f| matches!(f, Fotogramma::Previsto(p) if p.bidirezionale())), "i bidirezionali restano anche con le chiavi programmate");
        // un pezzo che non parte da una chiave non si apre
        let coda = Reel { fotogrammi: r.fotogrammi[1..3].to_vec(), fps: 12 }.in_byte();
        assert!(apri_reel(&coda).is_err());
        assert!(apri_reel(b"M4CF").is_err());
    }
}

#[cfg(test)]
mod robustezza {
    //! Pezzi corrotti, troncati o inventati: chi guarda non deve mai andare in panico (i pezzi arrivano dalla rete,
    //! firmati da chiunque). Un errore pulito sì, un crash dell'app no.
    use super::*;
    use crate::reel::prova::scena;
    use crate::reel::{codifica_reel, ParametriReel};
    use crate::{codifica, Parametri};

    fn caso(s: &mut u64) -> u64 {
        *s ^= *s << 13;
        *s ^= *s >> 7;
        *s ^= *s << 17;
        *s
    }

    fn senza_panico(nome: &str, pezzi: &[Vec<u8>], reel: bool) {
        let p: Vec<&[u8]> = pezzi.iter().map(|p| p.as_slice()).collect();
        let r = std::panic::catch_unwind(|| if reel { pezzi.iter().for_each(|p| { let _ = apri_reel(p); }) } else { let _ = apri_foto(&p); });
        assert!(r.is_ok(), "panico con {nome}");
    }

    #[test]
    fn pezzi_corrotti_troncati_o_inventati_non_fanno_panico() {
        let img = scena(48, 40, 0, 0, 0);
        let (c, _) = codifica(&img, Parametri { passo_dettaglio: 6, ..Parametri::default() }).unwrap();
        let foto = pezzi_foto(&c);
        let fotogrammi: Vec<_> = (0..5).map(|i| scena(48, 40, 2 * i, i, 0)).collect();
        let r = codifica_reel(&fotogrammi, ParametriReel { chiave_ogni: 4, bidirezionali: 1, ..ParametriReel::default() }).unwrap();
        let reel = pezzi_reel(&r);
        let mut s = 0x9e3779b97f4a7c15u64;
        for (nome, pezzi, e_reel) in [("foto", &foto, false), ("reel", &reel, true)] {
            senza_panico(nome, pezzi, e_reel);
            for giro in 0..400u32 {
                let mut m = pezzi.clone();
                let k = (caso(&mut s) as usize) % m.len();
                match giro % 4 {
                    0 => {
                        // un byte cambiato
                        let i = (caso(&mut s) as usize) % m[k].len();
                        m[k][i] ^= 1 << (caso(&mut s) % 8);
                    }
                    1 => {
                        // troncato
                        let l = (caso(&mut s) as usize) % m[k].len();
                        m[k].truncate(l);
                    }
                    2 => {
                        // dieci byte a caso dentro
                        for _ in 0..10 {
                            let i = (caso(&mut s) as usize) % m[k].len();
                            m[k][i] = caso(&mut s) as u8;
                        }
                    }
                    _ => {
                        // intestazione con misure enormi (solo nel primo pezzo): deve essere rifiutata senza allocare
                        if m[k].len() > 12 {
                            m[k][5] = 0xff;
                            m[k][6] = 0xff;
                            m[k][7] = 0xff;
                            m[k][8] = 0xff;
                        }
                    }
                }
                senza_panico(&format!("{nome} giro {giro}"), &m, e_reel);
            }
            // byte del tutto inventati
            let spazzatura: Vec<Vec<u8>> = (0..3).map(|_| (0..200).map(|_| caso(&mut s) as u8).collect()).collect();
            senza_panico(&format!("{nome} spazzatura"), &spazzatura, e_reel);
        }
    }
}
