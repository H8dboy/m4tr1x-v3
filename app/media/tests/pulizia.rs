//! I-16: dopo la pulizia nessun dato identificativo (GPS, modello, autore, programma, date, commenti) resta nel
//! file, il contenuto (immagine, audio, video) resta identico byte per byte, e i file strani si rifiutano.
//! I file di prova sono costruiti qui, con i "segreti" scritti dentro in chiaro per poterli cercare.

use m4tr1x_media::{mp4_solo_audio, ripulisci, tipo, Errore, Tipo, MASSIMO};

const SEGRETI: [&str; 6] = ["45.4642N", "iPhone 15 Pro", "Mario Rossi", "Lightroom", "2026:11:11", "SN-998877"];

fn contiene(b: &[u8], s: &str) -> bool {
    b.windows(s.len()).any(|w| w == s.as_bytes())
}
fn nessun_segreto(b: &[u8]) {
    for s in SEGRETI {
        assert!(!contiene(b, s), "nel file pulito resta {s:?}");
    }
}
fn seg(m: u8, dati: &[u8]) -> Vec<u8> {
    let mut v = vec![0xFF, m];
    v.extend_from_slice(&((dati.len() + 2) as u16).to_be_bytes());
    v.extend_from_slice(dati);
    v
}

/// EXIF con l'orientamento 6 e, in mezzo, i segreti come se fossero GPS, modello, autore.
fn exif(orientamento: u16) -> Vec<u8> {
    let mut t = b"II\x2a\0\x08\0\0\0".to_vec();
    t.extend_from_slice(&2u16.to_le_bytes());
    t.extend_from_slice(&0x0112u16.to_le_bytes());
    t.extend_from_slice(&3u16.to_le_bytes());
    t.extend_from_slice(&1u32.to_le_bytes());
    t.extend_from_slice(&orientamento.to_le_bytes());
    t.extend_from_slice(&[0, 0]);
    t.extend_from_slice(&0x0110u16.to_le_bytes()); // modello
    t.extend_from_slice(&2u16.to_le_bytes());
    t.extend_from_slice(&40u32.to_le_bytes());
    t.extend_from_slice(&38u32.to_le_bytes());
    t.extend_from_slice(&0u32.to_le_bytes());
    t.extend_from_slice(SEGRETI.join("|").as_bytes());
    let mut d = b"Exif\0\0".to_vec();
    d.extend_from_slice(&t);
    d
}

const IMMAGINE: &[u8] = b"\x12\x34DATI-COMPRESSI-DELL-IMMAGINE\x56\xFF\x00\x78";

fn jpeg_di_prova() -> Vec<u8> {
    let mut b = vec![0xFF, 0xD8];
    b.extend(seg(0xE0, b"JFIF\0\x01\x02\0\0\x01\0\x01\0\0"));
    b.extend(seg(0xE1, &exif(6)));
    b.extend(seg(0xE1, format!("http://ns.adobe.com/xap/1.0/\0<x:xmpmeta>{}</x:xmpmeta>", SEGRETI[3]).as_bytes()));
    b.extend(seg(0xE2, format!("ICC_PROFILE\0{}", SEGRETI[1]).as_bytes()));
    b.extend(seg(0xED, format!("Photoshop 3.0\0{}", SEGRETI[2]).as_bytes()));
    b.extend(seg(0xFE, SEGRETI[5].as_bytes()));
    b.extend(seg(0xDB, &[0u8; 65])); // tabella di quantizzazione
    b.extend(seg(0xC0, &[8, 0, 16, 0, 16, 1, 1, 0x11, 0]));
    b.extend(seg(0xDA, &[1, 1, 0, 0, 0x3F, 0]));
    b.extend_from_slice(IMMAGINE);
    b.extend_from_slice(&[0xFF, 0xD9]);
    b
}

#[test]
fn jpeg_senza_metadati_con_l_orientamento_e_l_immagine_identica() {
    let (t, p) = ripulisci(&jpeg_di_prova()).unwrap();
    assert_eq!(t, Tipo::Jpeg);
    nessun_segreto(&p);
    assert!(!contiene(&p, "xmpmeta") && !contiene(&p, "ICC_PROFILE") && !contiene(&p, "Photoshop"));
    assert!(p.ends_with(&[IMMAGINE, &[0xFF, 0xD9]].concat()), "i dati dell'immagine sono cambiati");
    assert!(contiene(&p, "JFIF"));
    // resta un EXIF con il solo orientamento (6): la foto non compare girata
    let i = p.windows(6).position(|w| w == b"Exif\0\0").expect("EXIF minimo");
    assert_eq!(&p[i + 6..i + 8], b"MM");
    assert_eq!(u16::from_be_bytes([p[i + 6 + 18], p[i + 6 + 19]]), 6);
    // resta esattamente: inizio, JFIF, EXIF minimo, tabelle, fotogramma, immagine, fine
    let atteso = 2 + (4 + 14) + (4 + 6 + 26) + (4 + 65) + (4 + 9) + (4 + 6) + IMMAGINE.len() + 2;
    assert_eq!(p.len(), atteso, "nel file pulito resta altro");
    // ripulire due volte dà lo stesso file
    assert_eq!(ripulisci(&p).unwrap().1, p);
}

#[test]
fn jpeg_senza_exif_resta_senza_exif() {
    let b: Vec<u8> = jpeg_di_prova().into_iter().collect();
    let senza: Vec<u8> = {
        let (_, p) = ripulisci(&b).unwrap();
        let i = p.windows(2).position(|w| w == [0xFF, 0xE1]).unwrap();
        let l = u16::from_be_bytes([p[i + 2], p[i + 3]]) as usize;
        [&p[..i], &p[i + 2 + l..]].concat()
    };
    let (_, p) = ripulisci(&senza).unwrap();
    assert!(!contiene(&p, "Exif"));
}

fn chunk(nome: &[u8; 4], dati: &[u8]) -> Vec<u8> {
    let mut v = (dati.len() as u32).to_be_bytes().to_vec();
    v.extend_from_slice(nome);
    v.extend_from_slice(dati);
    v.extend_from_slice(&[0xAB, 0xCD, 0xEF, 0x01]); // CRC (non controllato qui: lo copia com'è)
    v
}

#[test]
fn png_senza_testo_date_ed_exif() {
    let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
    b.extend(chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]));
    b.extend(chunk(b"tEXt", format!("Author\0{}", SEGRETI[2]).as_bytes()));
    b.extend(chunk(b"iTXt", format!("XML:com.adobe.xmp\0\0\0\0\0{}", SEGRETI[3]).as_bytes()));
    b.extend(chunk(b"eXIf", &exif(1)[6..]));
    b.extend(chunk(b"tIME", &[7, 0xEA, 11, 11, 10, 0, 0]));
    b.extend(chunk(b"zzZz", SEGRETI[5].as_bytes())); // blocco sconosciuto
    b.extend(chunk(b"sRGB", &[0]));
    b.extend(chunk(b"IDAT", IMMAGINE));
    b.extend(chunk(b"IEND", &[]));
    let (t, p) = ripulisci(&b).unwrap();
    assert_eq!(t, Tipo::Png);
    nessun_segreto(&p);
    for resta in ["IHDR", "IDAT", "sRGB", "IEND"] {
        assert!(contiene(&p, resta), "manca il blocco {resta}");
    }
    for via in [b"tEXt", b"iTXt", b"eXIf", b"tIME", b"zzZz"] {
        assert!(!contiene(&p, std::str::from_utf8(via).unwrap()), "resta il blocco {via:?}");
    }
    assert!(contiene(&p, "DATI-COMPRESSI-DELL-IMMAGINE"));
}

fn scatola(nome: &[u8; 4], dentro: &[u8]) -> Vec<u8> {
    let mut v = ((dentro.len() + 8) as u32).to_be_bytes().to_vec();
    v.extend_from_slice(nome);
    v.extend_from_slice(dentro);
    v
}

const VIDEO: &[u8] = b"FOTOGRAMMI-E-AUDIO-DEL-VIDEO-0123456789";

fn mp4_di_prova(versione_date: u8) -> Vec<u8> {
    let data = if versione_date == 1 { [vec![1, 0, 0, 0], vec![0xEE; 16], vec![0; 80]].concat() } else { [vec![0, 0, 0, 0], vec![0xEE; 8], vec![0; 88]].concat() };
    let gps = scatola(b"\xa9xyz", format!("+{}+009.1900/", SEGRETI[0]).as_bytes());
    let autore = scatola(b"\xa9ART", SEGRETI[2].as_bytes());
    let modello = scatola(b"\xa9mod", SEGRETI[1].as_bytes());
    let udta = scatola(b"udta", &[gps, autore, modello].concat());
    let meta = scatola(b"meta", &[vec![0, 0, 0, 0], scatola(b"ilst", format!("{}{}", SEGRETI[3], SEGRETI[4]).as_bytes())].concat());
    let tkhd = scatola(b"tkhd", &data);
    let mdhd = scatola(b"mdhd", &data);
    let hdlr = scatola(b"hdlr", b"\0\0\0\0\0\0\0\0vide\0\0\0\0\0\0\0\0\0\0\0\0\0");
    let trak = scatola(b"trak", &[tkhd, scatola(b"mdia", &[mdhd, hdlr].concat()), scatola(b"udta", SEGRETI[5].as_bytes())].concat());
    let moov = scatola(b"moov", &[scatola(b"mvhd", &data), trak, udta, meta].concat());
    let xmp = scatola(b"uuid", format!("BE7ACFCB97A942E89C71999491E3AFAC<x:xmpmeta>{}</x:xmpmeta>", SEGRETI[3]).as_bytes());
    [scatola(b"ftyp", b"isom\0\0\x02\0isomiso2mp41"), moov, xmp, scatola(b"mdat", VIDEO)].concat()
}

#[test]
fn mp4_senza_gps_autore_modello_date_stessa_lunghezza_e_video_identico() {
    for v in [0u8, 1] {
        let b = mp4_di_prova(v);
        let (t, p) = ripulisci(&b).unwrap();
        assert_eq!(t, Tipo::Mp4);
        nessun_segreto(&p);
        assert_eq!(p.len(), b.len(), "la lunghezza è cambiata: gli indici del video non tornerebbero più");
        let i = b.windows(VIDEO.len()).position(|w| w == VIDEO).unwrap();
        assert_eq!(&p[i..i + VIDEO.len()], VIDEO, "i dati del video sono cambiati o si sono spostati");
        assert!(!contiene(&p, "udta") && !contiene(&p, "uuid"), "resta un blocco di metadati");
        assert!(!p.windows(4).any(|w| w == [0xEE; 4]), "restano le date (versione {v})");
        assert!(contiene(&p, "moov") && contiene(&p, "mdat") && contiene(&p, "free"));
        assert!(!mp4_solo_audio(&p));
    }
}

#[test]
fn mp4_rovinati_si_rifiutano_senza_panico() {
    let b = mp4_di_prova(0);
    // lunghezza di un blocco oltre la fine
    let mut x = b.clone();
    let i = x.windows(4).position(|w| w == b"moov").unwrap() - 4;
    x[i..i + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(ripulisci(&x), Err(Errore::Rovinato(_))));
    // tagliato a metà, senza moov
    let senza_moov = [scatola(b"ftyp", b"isom\0\0\x02\0"), scatola(b"mdat", VIDEO)].concat();
    assert!(matches!(ripulisci(&senza_moov), Err(Errore::Rovinato(_))));
    for n in [9, 20, 50, b.len() - 3] {
        let _ = ripulisci(&b[..n]); // mai un panico
    }
}

#[test]
fn mp3_senza_id3_ne_ape() {
    let audio = [vec![0xFF, 0xFB, 0x90, 0x64], b"AUDIO-DEL-BRANO".to_vec()].concat();
    let mut id3 = b"ID3\x04\x00\x00".to_vec();
    let corpo = format!("TPE1\0\0\0\x20\0\0\x03{}TALB{}", SEGRETI[2], SEGRETI[4]);
    let n = corpo.len();
    id3.extend_from_slice(&[(n >> 21 & 0x7F) as u8, (n >> 14 & 0x7F) as u8, (n >> 7 & 0x7F) as u8, (n & 0x7F) as u8]);
    id3.extend_from_slice(corpo.as_bytes());
    let mut v1 = b"TAG".to_vec();
    v1.extend_from_slice(format!("{:<125}", SEGRETI[5]).as_bytes());
    let mut ape_corpo = format!("{}", SEGRETI[3]).into_bytes();
    ape_corpo.resize(40, 0);
    let mut ape_piede = b"APETAGEX".to_vec();
    ape_piede.extend_from_slice(&2000u32.to_le_bytes());
    ape_piede.extend_from_slice(&((ape_corpo.len() + 32) as u32).to_le_bytes());
    ape_piede.extend_from_slice(&1u32.to_le_bytes());
    ape_piede.extend_from_slice(&0u32.to_le_bytes());
    ape_piede.extend_from_slice(&[0u8; 8]);
    let b = [id3, audio.clone(), ape_corpo, ape_piede, v1].concat();
    let (t, p) = ripulisci(&b).unwrap();
    assert_eq!(t, Tipo::Mp3);
    nessun_segreto(&p);
    assert_eq!(p, audio, "dell'MP3 deve restare solo l'audio");
}

#[test]
fn solo_i_tipi_ammessi_e_entro_la_dimensione() {
    assert_eq!(ripulisci(b"GIF89a....").unwrap_err(), Errore::TipoNonAmmesso);
    assert_eq!(ripulisci(b"%PDF-1.7").unwrap_err(), Errore::TipoNonAmmesso);
    assert_eq!(ripulisci(b"<svg onload=alert(1)>").unwrap_err(), Errore::TipoNonAmmesso);
    assert_eq!(ripulisci(&[]).unwrap_err(), Errore::TipoNonAmmesso);
    // HEIC/AVIF: stessa struttura di un MP4, ma l'immagine sta in `meta`: mai trattati come video
    for marca in [b"heic", b"heix", b"mif1", b"avif"] {
        let f = [scatola(b"ftyp", &[marca.as_slice(), &[0u8; 4], b"mif1heic"].concat()), scatola(b"meta", b"IMMAGINE"), scatola(b"mdat", VIDEO)].concat();
        assert_eq!(ripulisci(&f).unwrap_err(), Errore::TipoNonAmmesso, "{marca:?} accettato come video");
    }
    assert_eq!(tipo(b"\x89PNG\r\n\x1a\n"), Some(Tipo::Png));
    let grande = vec![0xFFu8; MASSIMO + 1];
    assert_eq!(ripulisci(&grande).unwrap_err(), Errore::TroppoGrande);
    // un JPEG fatto solo di metadati, senza immagine
    let mut solo_meta = vec![0xFF, 0xD8];
    solo_meta.extend(seg(0xE1, &exif(6)));
    solo_meta.extend_from_slice(&[0xFF, 0xD9]);
    assert!(matches!(ripulisci(&solo_meta), Err(Errore::Rovinato(_))));
}

/// Una traccia con il suo hdlr e una tabella dei campioni (stsz, stsc, stco) che punta dentro mdat.
fn traccia(handler: &[u8; 4], offset_campione: u32, campione: usize) -> Vec<u8> {
    let hdlr = scatola(b"hdlr", &[b"\0\0\0\0\0\0\0\0".as_slice(), handler, &[0u8; 12], b"Core Media Metadata\0"].concat());
    let stsz = scatola(b"stsz", &[vec![0, 0, 0, 0], (campione as u32).to_be_bytes().to_vec(), 1u32.to_be_bytes().to_vec()].concat());
    let stsc = scatola(b"stsc", &[vec![0, 0, 0, 0], 1u32.to_be_bytes().to_vec(), 1u32.to_be_bytes().to_vec(), 1u32.to_be_bytes().to_vec(), 1u32.to_be_bytes().to_vec()].concat());
    let stco = scatola(b"stco", &[vec![0, 0, 0, 0], 1u32.to_be_bytes().to_vec(), offset_campione.to_be_bytes().to_vec()].concat());
    let stbl = scatola(b"stbl", &[stsz, stsc, stco].concat());
    let mdia = scatola(b"mdia", &[scatola(b"mdhd", &[0u8; 24]), hdlr, scatola(b"minf", &stbl)].concat());
    scatola(b"trak", &[scatola(b"tkhd", &[0u8; 84]), mdia].concat())
}

#[test]
fn mp4_le_tracce_di_metadati_spariscono_con_i_loro_campioni() {
    // come un video dell'iPhone: traccia video + traccia "volti riconosciuti" con i campioni dentro mdat
    let volti = b"com.apple.quicktime.detected-face FACE-ID-123 bounds";
    let ftyp = scatola(b"ftyp", b"isom\0\0\x02\0isom");
    // la lunghezza di moov non dipende dagli offset: si calcola con un offset finto, poi si rifà
    let moov_finto = scatola(b"moov", &[traccia(b"vide", 0, VIDEO.len()), traccia(b"meta", 0, volti.len())].concat());
    let inizio_mdat = ftyp.len() + moov_finto.len() + 8;
    let (off_video, off_volti) = (inizio_mdat as u32, (inizio_mdat + VIDEO.len()) as u32);
    let moov = scatola(b"moov", &[traccia(b"vide", off_video, VIDEO.len()), traccia(b"meta", off_volti, volti.len())].concat());
    let b = [ftyp, moov, scatola(b"mdat", &[VIDEO, volti.as_slice()].concat())].concat();
    assert_eq!(&b[off_volti as usize..off_volti as usize + 4], b"com.");
    let (_, p) = ripulisci(&b).unwrap();
    assert_eq!(p.len(), b.len());
    assert!(!contiene(&p, "FACE-ID-123") && !contiene(&p, "detected-face"), "restano i volti riconosciuti");
    assert_eq!(p.windows(4).filter(|w| *w == b"meta").count(), 0, "resta la traccia di metadati");
    assert_eq!(&p[off_video as usize..off_video as usize + VIDEO.len()], VIDEO, "il video è cambiato");
    assert!(contiene(&p, "vide"), "la traccia video è sparita");
    // una traccia di metadati che punta fuori dal file: rifiutato, non pulito a metà
    let mut rotto = b.clone();
    let k = rotto.windows(4).rposition(|w| w == b"stco").unwrap();
    rotto[k + 12..k + 16].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(ripulisci(&rotto), Err(Errore::Rovinato(_))));
}
