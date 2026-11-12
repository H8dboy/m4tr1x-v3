//! Pulizia dei metadati SUL DISPOSITIVO, prima che un file parta (I-16).
//!
//! Un file caricato su M4TR1X non deve portarsi dietro chi l'ha fatto, dove e con cosa: GPS, modello del
//! telefono, numero di serie, autore, programma, date, commenti. Qui si accettano SOLO i tipi elencati in
//! [`Tipo`], riconosciuti dai primi byte (mai dall'estensione), e per ognuno si tiene solo ciò che serve a
//! mostrare il contenuto. Tutto il resto si toglie. Un file che non si riesce a leggere fino in fondo si
//! rifiuta: meglio non caricare che caricare un file pulito a metà.
//!
//! - JPEG: si tengono solo i segmenti dell'immagine (più JFIF e Adobe). Dell'EXIF resta SOLO l'orientamento,
//!   riscritto da zero (altrimenti le foto del telefono comparirebbero girate).
//! - PNG: si tengono solo i blocchi che servono a disegnare; testo, data, EXIF e blocchi sconosciuti via.
//! - MP4/MOV/M4A: i blocchi `udta`, `meta`, `uuid` (dove stanno GPS, autore, programma, XMP) diventano `free`
//!   della STESSA lunghezza, riempiti di zeri; le date di creazione e modifica si azzerano. La lunghezza del
//!   file non cambia: gli indici dei dati audio e video restano giusti senza riscriverli.
//! - MP3: via ID3v2 in testa, ID3v1 (e la sua estensione) e APEv2 in coda.

/// Dimensione massima di un file da pulire (come `MASSIMO_FILE` della scheda: 1 GB, poi a pezzi da 4 MB verso il
/// nodo). Limite dichiarato: la pulizia lavora sul file intero in memoria.
pub const MASSIMO: usize = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    Jpeg,
    Png,
    /// MP4, MOV, M4A (stessa struttura a blocchi)
    Mp4,
    Mp3,
}

impl Tipo {
    pub fn mime(self, audio: bool) -> &'static str {
        match self {
            Tipo::Jpeg => "image/jpeg",
            Tipo::Png => "image/png",
            Tipo::Mp4 if audio => "audio/mp4",
            Tipo::Mp4 => "video/mp4",
            Tipo::Mp3 => "audio/mpeg",
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Errore {
    #[error("tipo di file non ammesso: si caricano solo foto JPEG o PNG, video MP4 o MOV, audio MP3 o M4A (le foto HEIC dell'iPhone: Impostazioni › Fotocamera › Formati › Più compatibile)")]
    TipoNonAmmesso,
    #[error("file troppo grande (massimo 1 GB)")]
    TroppoGrande,
    #[error("file rovinato o incompleto: {0}")]
    Rovinato(&'static str),
}

type R<T> = Result<T, Errore>;

/// Marche (`ftyp`) di MP4, MOV e M4A ammesse. HEIC, HEIF, AVIF (heic, heix, mif1, msf1, avif…) NO.
const MARCHE_MP4: [&[u8; 4]; 12] = [b"isom", b"iso2", b"iso4", b"iso5", b"iso6", b"mp41", b"mp42", b"qt  ", b"M4A ", b"M4V ", b"avc1", b"3gp4"];

/// Riconosce il tipo dai primi byte.
pub fn tipo(b: &[u8]) -> Option<Tipo> {
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(Tipo::Jpeg)
    } else if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(Tipo::Png)
    } else if b.len() >= 12 && &b[4..8] == b"ftyp" && MARCHE_MP4.iter().any(|m| *m == &b[8..12]) {
        // solo video e audio: HEIC/AVIF hanno la stessa struttura ma l'IMMAGINE sta dentro `meta`, che qui si azzera
        Some(Tipo::Mp4)
    } else if b.starts_with(b"ID3") || (b.len() >= 2 && b[0] == 0xFF && b[1] & 0xE0 == 0xE0) {
        Some(Tipo::Mp3)
    } else {
        None
    }
}

/// Pulisce `b` secondo il suo tipo. Restituisce il tipo e il file pulito.
pub fn ripulisci(b: &[u8]) -> R<(Tipo, Vec<u8>)> {
    if b.len() > MASSIMO {
        return Err(Errore::TroppoGrande);
    }
    let t = tipo(b).ok_or(Errore::TipoNonAmmesso)?;
    let out = match t {
        Tipo::Jpeg => jpeg(b)?,
        Tipo::Png => png(b)?,
        Tipo::Mp4 => mp4(b)?,
        Tipo::Mp3 => mp3(b)?,
    };
    Ok((t, out))
}

/// Un file MP4 è solo audio (M4A) se non ha tracce video: serve a scegliere il tipo MIME.
pub fn mp4_solo_audio(b: &[u8]) -> bool {
    !finestra(b, b"vide") && finestra(b, b"soun")
}

fn finestra(b: &[u8], ago: &[u8]) -> bool {
    b.windows(ago.len()).any(|w| w == ago)
}

// ─── JPEG ────────────────────────────────────────────────────────────────────

fn jpeg(b: &[u8]) -> R<Vec<u8>> {
    let no = Errore::Rovinato;
    let mut out = Vec::with_capacity(b.len());
    out.extend_from_slice(&[0xFF, 0xD8]);
    let mut i = 2;
    let mut orientamento: Option<u16> = None;
    let mut scritto_orientamento = false;
    loop {
        // i marcatori possono essere preceduti da byte 0xFF di riempimento
        while i < b.len() && b[i] == 0xFF && i + 1 < b.len() && b[i + 1] == 0xFF {
            i += 1;
        }
        if i + 4 > b.len() || b[i] != 0xFF {
            return Err(no("JPEG senza immagine"));
        }
        let m = b[i + 1];
        if m == 0xD9 {
            return Err(no("JPEG finito prima dell'immagine"));
        }
        let lung = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if lung < 2 || i + 2 + lung > b.len() {
            return Err(no("segmento JPEG oltre la fine"));
        }
        let seg = &b[i..i + 2 + lung];
        let dati = &seg[4..];
        match m {
            // inizio dell'immagine: da qui in poi sono solo dati compressi, si copia tutto com'è
            0xDA => {
                if let (Some(o), false) = (orientamento, scritto_orientamento) {
                    out.extend_from_slice(&exif_solo_orientamento(o));
                }
                out.extend_from_slice(&b[i..]);
                return Ok(out);
            }
            // JFIF (APP0) e Adobe (APP14): solo parametri dell'immagine, niente dati personali
            0xE0 if dati.starts_with(b"JFIF\0") => out.extend_from_slice(seg),
            0xEE if dati.starts_with(b"Adobe") => out.extend_from_slice(seg),
            // EXIF: se ne legge SOLO l'orientamento, il resto si butta
            0xE1 if dati.starts_with(b"Exif\0\0") => {
                if orientamento.is_none() {
                    orientamento = leggi_orientamento(&dati[6..]);
                }
            }
            // ogni altro APPn (XMP, ICC, IPTC, produttori) e i commenti: via
            0xE0..=0xEF | 0xFE => {}
            // tabelle, intestazione del fotogramma e il resto della struttura: si tengono
            _ => {
                if !scritto_orientamento {
                    if let Some(o) = orientamento {
                        // l'EXIF minimo va prima delle tabelle (posizione di un APP1)
                        out.extend_from_slice(&exif_solo_orientamento(o));
                        scritto_orientamento = true;
                    }
                }
                out.extend_from_slice(seg)
            }
        }
        i += 2 + lung;
    }
}

/// Orientamento (tag 0x0112) dal TIFF dentro l'EXIF, se c'è e vale 1..=8.
fn leggi_orientamento(t: &[u8]) -> Option<u16> {
    if t.len() < 8 {
        return None;
    }
    let le = match &t[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_ = |o: usize| t.get(o..o + 2).map(|x| if le { u16::from_le_bytes([x[0], x[1]]) } else { u16::from_be_bytes([x[0], x[1]]) });
    let u32_ = |o: usize| t.get(o..o + 4).map(|x| if le { u32::from_le_bytes([x[0], x[1], x[2], x[3]]) } else { u32::from_be_bytes([x[0], x[1], x[2], x[3]]) });
    let ifd = u32_(4)? as usize;
    let n = u16_(ifd)? as usize;
    for k in 0..n.min(512) {
        let e = ifd + 2 + 12 * k;
        if u16_(e)? == 0x0112 {
            let v = u16_(e + 8)?;
            return (1..=8).contains(&v).then_some(v);
        }
    }
    None
}

/// Un segmento APP1 EXIF scritto da zero con UN solo dato: l'orientamento.
fn exif_solo_orientamento(o: u16) -> Vec<u8> {
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"MM\0\x2a\0\0\0\x08"); // big endian, primo IFD a 8
    tiff.extend_from_slice(&1u16.to_be_bytes()); // una voce
    tiff.extend_from_slice(&0x0112u16.to_be_bytes());
    tiff.extend_from_slice(&3u16.to_be_bytes()); // SHORT
    tiff.extend_from_slice(&1u32.to_be_bytes());
    tiff.extend_from_slice(&o.to_be_bytes());
    tiff.extend_from_slice(&[0, 0]);
    tiff.extend_from_slice(&0u32.to_be_bytes()); // nessun IFD dopo
    let mut seg = vec![0xFF, 0xE1];
    seg.extend_from_slice(&((2 + 6 + tiff.len()) as u16).to_be_bytes());
    seg.extend_from_slice(b"Exif\0\0");
    seg.extend_from_slice(&tiff);
    seg
}

// ─── PNG ─────────────────────────────────────────────────────────────────────

/// Blocchi che servono a disegnare (anche le animazioni APNG). Tutto il resto si toglie.
const PNG_AMMESSI: [&[u8; 4]; 13] = [b"IHDR", b"PLTE", b"IDAT", b"IEND", b"tRNS", b"gAMA", b"cHRM", b"sRGB", b"sBIT", b"bKGD", b"acTL", b"fcTL", b"fdAT"];

fn png(b: &[u8]) -> R<Vec<u8>> {
    let no = Errore::Rovinato;
    let mut out = b[..8].to_vec();
    let mut i = 8;
    loop {
        if i + 12 > b.len() {
            return Err(no("PNG senza fine"));
        }
        let lung = u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize;
        let fine = i.checked_add(12 + lung).ok_or(no("blocco PNG"))?;
        if fine > b.len() {
            return Err(no("blocco PNG oltre la fine"));
        }
        let nome = &b[i + 4..i + 8];
        if PNG_AMMESSI.iter().any(|a| a.as_slice() == nome) {
            out.extend_from_slice(&b[i..fine]);
        }
        if nome == b"IEND" {
            return Ok(out);
        }
        i = fine;
    }
}

// ─── MP4 / MOV / M4A ─────────────────────────────────────────────────────────

/// Blocchi in cui si scende (contengono altri blocchi).
const MP4_CONTENITORI: [&[u8; 4]; 9] = [b"moov", b"trak", b"mdia", b"minf", b"stbl", b"edts", b"mvex", b"moof", b"traf"];
/// Blocchi con dati personali: diventano `free` della stessa lunghezza.
const MP4_VIA: [&[u8; 4]; 4] = [b"udta", b"meta", b"uuid", b"XMP_"];
/// Blocchi con le date (versione 0: 4 byte, versione 1: 8 byte, creazione e modifica dopo versione e flag).
const MP4_DATE: [&[u8; 4]; 3] = [b"mvhd", b"tkhd", b"mdhd"];

fn mp4(b: &[u8]) -> R<Vec<u8>> {
    let mut out = b.to_vec();
    let mut trovato_moov = false;
    blocchi(&mut out, 0, b.len(), 0, &mut trovato_moov)?;
    if !trovato_moov {
        return Err(Errore::Rovinato("MP4 senza indice (moov)"));
    }
    Ok(out)
}

fn blocchi(b: &mut [u8], mut i: usize, fine: usize, profondita: u8, moov: &mut bool) -> R<()> {
    let no = Errore::Rovinato;
    if profondita > 12 {
        return Err(no("blocchi MP4 troppo annidati"));
    }
    while i < fine {
        if i + 8 > fine {
            return Err(no("blocco MP4 tagliato"));
        }
        let corta = u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as u64;
        let nome: [u8; 4] = [b[i + 4], b[i + 5], b[i + 6], b[i + 7]];
        let (lung, testa) = match corta {
            0 => ((fine - i) as u64, 8usize), // fino alla fine del contenitore
            1 => {
                if i + 16 > fine {
                    return Err(no("blocco MP4 tagliato"));
                }
                let mut x = [0u8; 8];
                x.copy_from_slice(&b[i + 8..i + 16]);
                (u64::from_be_bytes(x), 16usize)
            }
            n => (n, 8usize),
        };
        if lung < testa as u64 || lung > (fine - i) as u64 {
            return Err(no("lunghezza di un blocco MP4"));
        }
        let lung = lung as usize;
        if nome == *b"moov" {
            *moov = true;
        }
        if nome == *b"trak" && !traccia_audio_video(b, i + testa, i + lung)? {
            // traccia di metadati (iPhone: volti riconosciuti, orientamento…), capitoli, testo: via la traccia E i
            // suoi campioni dentro i dati, sempre a lunghezza invariata
            for (da, n) in campioni_della_traccia(b, i + testa, i + lung)? {
                b[da..da + n].fill(0);
            }
            b[i + 4..i + 8].copy_from_slice(b"free");
            b[i + testa..i + lung].fill(0);
        } else if MP4_VIA.iter().any(|v| **v == nome) {
            // stessa lunghezza, nome `free`, contenuto azzerato: gli indici dei dati restano giusti
            b[i + 4..i + 8].copy_from_slice(b"free");
            b[i + testa..i + lung].fill(0);
        } else if MP4_DATE.iter().any(|v| **v == nome) {
            let v = *b.get(i + testa).ok_or(no("blocco delle date"))?;
            let n = if v == 1 { 16 } else { 8 };
            let da = i + testa + 4;
            if da + n > i + lung {
                return Err(no("blocco delle date tagliato"));
            }
            b[da..da + n].fill(0);
        } else if MP4_CONTENITORI.iter().any(|v| **v == nome) {
            blocchi(b, i + testa, i + lung, profondita + 1, moov)?;
        }
        i += lung;
    }
    Ok(())
}

/// I figli diretti di un blocco: (nome, inizio, testa, lunghezza). Errore se una lunghezza non torna.
fn figli(b: &[u8], mut i: usize, fine: usize) -> R<Vec<([u8; 4], usize, usize, usize)>> {
    let no = Errore::Rovinato;
    let mut v = vec![];
    while i < fine {
        if i + 8 > fine {
            return Err(no("blocco MP4 tagliato"));
        }
        let corta = u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as u64;
        let nome = [b[i + 4], b[i + 5], b[i + 6], b[i + 7]];
        let (lung, testa) = match corta {
            0 => ((fine - i) as u64, 8),
            1 if i + 16 <= fine => (u64::from_be_bytes(b[i + 8..i + 16].try_into().expect("8 byte")), 16),
            1 => return Err(no("blocco MP4 tagliato")),
            n => (n, 8),
        };
        if lung < testa as u64 || lung > (fine - i) as u64 {
            return Err(no("lunghezza di un blocco MP4"));
        }
        v.push((nome, i, testa, lung as usize));
        i += lung as usize;
    }
    Ok(v)
}

fn figlio(b: &[u8], da: usize, fine: usize, nome: &[u8; 4]) -> R<Option<(usize, usize)>> {
    Ok(figli(b, da, fine)?.into_iter().find(|f| f.0 == *nome).map(|f| (f.1 + f.2, f.1 + f.3)))
}

/// Il contenuto di trak → mdia → hdlr dice che tipo di traccia è: si tengono solo video e audio.
fn traccia_audio_video(b: &[u8], da: usize, fine: usize) -> R<bool> {
    let no = Errore::Rovinato;
    let (m0, m1) = figlio(b, da, fine, b"mdia")?.ok_or(no("traccia senza mdia"))?;
    let (h0, h1) = figlio(b, m0, m1, b"hdlr")?.ok_or(no("traccia senza hdlr"))?;
    let t = b.get(h0 + 8..h0 + 12).filter(|_| h0 + 12 <= h1).ok_or(no("hdlr tagliato"))?;
    Ok(t == b"vide" || t == b"soun")
}

fn u32_a(b: &[u8], i: usize) -> R<u32> {
    b.get(i..i + 4).map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]])).ok_or(Errore::Rovinato("tabella MP4 tagliata"))
}

/// Dove stanno nel file i campioni di una traccia: (inizio, lunghezza) da stsz, stsc e stco/co64.
/// Ogni intervallo deve stare dentro il file: altrimenti il file si rifiuta.
fn campioni_della_traccia(b: &[u8], da: usize, fine: usize) -> R<Vec<(usize, usize)>> {
    let no = Errore::Rovinato;
    let (m0, m1) = figlio(b, da, fine, b"mdia")?.ok_or(no("traccia senza mdia"))?;
    let (i0, i1) = figlio(b, m0, m1, b"minf")?.ok_or(no("traccia senza minf"))?;
    let (s0, s1) = figlio(b, i0, i1, b"stbl")?.ok_or(no("traccia senza stbl"))?;
    // offset dei blocchi di campioni
    let offset: Vec<u64> = if let Some((c0, _)) = figlio(b, s0, s1, b"stco")? {
        let n = u32_a(b, c0 + 4)? as usize;
        (0..n.min(10_000_000)).map(|k| u32_a(b, c0 + 8 + 4 * k).map(u64::from)).collect::<R<_>>()?
    } else if let Some((c0, _)) = figlio(b, s0, s1, b"co64")? {
        let n = u32_a(b, c0 + 4)? as usize;
        (0..n.min(10_000_000)).map(|k| Ok((u32_a(b, c0 + 8 + 8 * k)? as u64) << 32 | u32_a(b, c0 + 12 + 8 * k)? as u64)).collect::<R<_>>()?
    } else {
        return Ok(vec![]); // nessun campione
    };
    // dimensioni dei campioni
    let (z0, _) = figlio(b, s0, s1, b"stsz")?.ok_or(no("traccia senza stsz"))?;
    let fissa = u32_a(b, z0 + 4)? as usize;
    let quanti = u32_a(b, z0 + 8)? as usize;
    if quanti > 10_000_000 {
        return Err(no("troppi campioni"));
    }
    let misura = |k: usize| -> R<usize> { if fissa != 0 { Ok(fissa) } else { u32_a(b, z0 + 12 + 4 * k).map(|x| x as usize) } };
    // campioni per blocco
    let (p0, _) = figlio(b, s0, s1, b"stsc")?.ok_or(no("traccia senza stsc"))?;
    let voci = u32_a(b, p0 + 4)? as usize;
    let stsc: Vec<(usize, usize)> = (0..voci.min(10_000_000)).map(|k| Ok((u32_a(b, p0 + 8 + 12 * k)? as usize, u32_a(b, p0 + 12 + 12 * k)? as usize))).collect::<R<_>>()?;
    let mut out = vec![];
    let mut campione = 0usize;
    for (c, &inizio) in offset.iter().enumerate() {
        let n_blocco = c + 1;
        let per_blocco = stsc.iter().rev().find(|(primo, _)| *primo <= n_blocco).map(|v| v.1).unwrap_or(0);
        let mut pos = usize::try_from(inizio).map_err(|_| no("offset MP4"))?;
        for _ in 0..per_blocco {
            if campione >= quanti {
                break;
            }
            let n = misura(campione)?;
            let fine_c = pos.checked_add(n).filter(|&f| f <= b.len()).ok_or(no("campione oltre la fine del file"))?;
            out.push((pos, n));
            pos = fine_c;
            campione += 1;
        }
    }
    Ok(out)
}

// ─── MP3 ─────────────────────────────────────────────────────────────────────

fn mp3(b: &[u8]) -> R<Vec<u8>> {
    let no = Errore::Rovinato;
    let mut inizio = 0usize;
    // ID3v2 (anche più di uno di seguito)
    while b.len() >= inizio + 10 && &b[inizio..inizio + 3] == b"ID3" {
        let s = &b[inizio + 6..inizio + 10];
        if s.iter().any(|x| x & 0x80 != 0) {
            return Err(no("intestazione ID3"));
        }
        let mut lung = ((s[0] as usize) << 21 | (s[1] as usize) << 14 | (s[2] as usize) << 7 | s[3] as usize) + 10;
        if b[inizio + 5] & 0x10 != 0 {
            lung += 10; // piè di pagina
        }
        inizio = inizio.checked_add(lung).filter(|&x| x <= b.len()).ok_or(no("ID3 oltre la fine"))?;
    }
    let mut fine = b.len();
    loop {
        if fine >= inizio + 32 && &b[fine - 32..fine - 24] == b"APETAGEX" {
            let l = u32::from_le_bytes([b[fine - 20], b[fine - 19], b[fine - 18], b[fine - 17]]) as usize;
            let con_testa = u32::from_le_bytes([b[fine - 12], b[fine - 11], b[fine - 10], b[fine - 9]]) & 0x8000_0000 != 0;
            let tot = l + if con_testa { 32 } else { 0 };
            fine = fine.checked_sub(tot).filter(|&x| x >= inizio).ok_or(no("APE oltre l'inizio"))?;
        } else if fine >= inizio + 128 && &b[fine - 128..fine - 125] == b"TAG" {
            fine -= 128;
            if fine >= inizio + 227 && &b[fine - 227..fine - 223] == b"TAG+" {
                fine -= 227;
            }
        } else {
            break;
        }
    }
    let audio = &b[inizio..fine];
    if audio.len() < 4 || audio[0] != 0xFF || audio[1] & 0xE0 != 0xE0 {
        return Err(no("MP3 senza audio dopo i tag"));
    }
    Ok(audio.to_vec())
}
