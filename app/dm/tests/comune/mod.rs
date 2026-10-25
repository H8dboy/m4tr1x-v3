//! Utenti di prova (semi PUBBLICI di prova, non segreti) e fonti di casualità per le prove di F5.
#![allow(dead_code)]

use m4tr1x_dm::Entropia;
use m4tr1x_identity::{deriva, seme_da_parole, ChiaveRuolo, Ruolo};

pub const ORA: u64 = 1_760_000_000;

fn parole(a: &str, ultima: &str) -> String {
    [a; 23].join(" ") + " " + ultima
}

/// Anna: seme "abandon … art" (lo stesso delle altre prove). Bruno: "zoo … vote". Carla: "legal winner thank … " non serve: si deriva una versione 2.
pub struct Utente {
    pub identita: ChiaveRuolo,
    pub dm: ChiaveRuolo,
}

fn utente(p: &str, versione_dm: u32) -> Utente {
    let seme = seme_da_parole(p, "").unwrap();
    Utente { identita: deriva(&seme, Ruolo::Identity, 1).unwrap(), dm: deriva(&seme, Ruolo::Dm, versione_dm).unwrap() }
}

pub fn anna() -> Utente {
    utente(&parole("abandon", "art"), 1)
}
pub fn anna_dm_v2() -> Utente {
    utente(&parole("abandon", "art"), 2)
}
pub fn bruno() -> Utente {
    utente(&parole("zoo", "vote"), 1)
}

/// Casualità riproducibile: contatore (ogni chiamata dà byte diversi), per le prove che devono ripetersi.
pub struct Contatore(pub u64);
impl Entropia for Contatore {
    fn riempi(&mut self, dove: &mut [u8]) {
        for b in dove.iter_mut() {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            *b = (self.0 >> 33) as u8;
        }
    }
}
