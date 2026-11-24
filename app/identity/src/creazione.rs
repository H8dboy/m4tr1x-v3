//! Creazione e recupero dell'identità (I-58).
//!
//! Creazione: si generano 24 parole, la finestra del seme le mostra, poi l'utente deve
//! riscrivere 3 parole in posizioni scelte QUI a caso. Solo se sono giuste si ricavano
//! le chiavi e si salvano identity, dm e wallet. Le parole non si salvano mai e si
//! cancellano dalla memoria appena la creazione finisce (o viene abbandonata).
//! Recupero: dalle 24 parole si ricavano e si salvano le stesse chiavi.

use crate::portachiavi::{self, Portachiavi, RUOLI_SALVATI};
use crate::{deriva, genera_parole, seme_da_parole, IdentityError, Parole, Ruolo};

/// Quante parole l'utente deve riscrivere (decisione del fondatore, 16/10/2026).
pub const POSIZIONI_DA_VERIFICARE: usize = 3;

/// Chiavi pubbliche dell'identità: l'unica cosa che esce verso la finestra principale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentitaPubblica {
    pub identity: String,
    pub dm: String,
    /// impronta (sha256) della chiave pubblica ML-DSA-65, 1952 byte sono troppi da mostrare
    pub wallet_impronta: String,
}

pub struct CreazioneInCorso {
    parole: Parole,
    posizioni: [usize; POSIZIONI_DA_VERIFICARE],
}

fn casuale_sotto(n: usize) -> Result<usize, IdentityError> {
    // rifiuto dei valori in coda: niente distorsione verso le prime posizioni
    let limite = u32::MAX - (u32::MAX % n as u32);
    loop {
        let mut b = [0u8; 4];
        getrandom::fill(&mut b).map_err(|_| IdentityError::Casuale)?;
        let v = u32::from_le_bytes(b);
        if v < limite {
            return Ok((v % n as u32) as usize);
        }
    }
}

impl CreazioneInCorso {
    pub fn nuova() -> Result<Self, IdentityError> {
        let parole = genera_parole()?;
        let n = parole.elenco().len();
        let mut posizioni = [0usize; POSIZIONI_DA_VERIFICARE];
        let mut scelte = 0;
        while scelte < POSIZIONI_DA_VERIFICARE {
            let p = casuale_sotto(n)?;
            if !posizioni[..scelte].contains(&p) {
                posizioni[scelte] = p;
                scelte += 1;
            }
        }
        posizioni.sort_unstable();
        Ok(Self { parole, posizioni })
    }

    /// Le parole da mostrare (solo alla finestra del seme).
    pub fn parole(&self) -> Vec<&str> {
        self.parole.elenco()
    }

    /// Posizioni (da 0) che l'utente deve riscrivere. Non sono un segreto.
    pub fn posizioni(&self) -> [usize; POSIZIONI_DA_VERIFICARE] {
        self.posizioni
    }

    /// Controlla le 3 parole; se giuste salva le chiavi e restituisce le pubbliche.
    /// Se sbagliate non salva niente (si può riprovare).
    pub fn conferma(&self, risposte: &[String], p: &dyn Portachiavi) -> Result<IdentitaPubblica, IdentityError> {
        let elenco = self.parole.elenco();
        let giuste = risposte.len() == POSIZIONI_DA_VERIFICARE
            && self.posizioni.iter().zip(risposte).all(|(&i, r)| r.trim().to_lowercase() == elenco[i]);
        if !giuste {
            return Err(IdentityError::ConfermaSbagliata);
        }
        salva_da_parole(&elenco.join(" "), p)
    }
}

/// Recupero: le 24 parole → stesse chiavi di prima, salvate nel portachiavi.
pub fn recupera(parole: &str, p: &dyn Portachiavi) -> Result<IdentitaPubblica, IdentityError> {
    salva_da_parole(parole, p)
}

fn salva_da_parole(parole: &str, p: &dyn Portachiavi) -> Result<IdentitaPubblica, IdentityError> {
    let seme = seme_da_parole(parole, "")?;
    let mut pubbliche = Vec::with_capacity(3);
    // prima si ricava tutto, poi si scrive: o tutte e tre le chiavi o nessuna
    let chiavi = RUOLI_SALVATI.iter().map(|&r| deriva(&seme, r, 1)).collect::<Result<Vec<_>, _>>()?;
    // un'identità DIVERSA da quella già nel portachiavi non eredita le sue chiavi DM ruotate; la stessa (recupero
    // sullo stesso dispositivo) le tiene, così i messaggi recenti restano leggibili
    let stessa = matches!(portachiavi::carica(p, Ruolo::Identity, 1)?, Some(k) if k.pubblica_hex() == chiavi[0].pubblica_hex());
    if !stessa {
        portachiavi::dimentica_dm_ruotate(p)?;
    }
    for k in &chiavi {
        portachiavi::salva(p, k)?;
        pubbliche.push(k.pubblica_hex());
    }
    Ok(IdentitaPubblica { identity: pubbliche[0].clone(), dm: pubbliche[1].clone(), wallet_impronta: crate::impronta_wallet(&pubbliche[2]) })
}

/// Identità già salvata, se c'è (solo chiavi pubbliche).
pub fn identita_salvata(p: &dyn Portachiavi) -> Result<Option<IdentitaPubblica>, IdentityError> {
    let mut pubbliche = Vec::with_capacity(3);
    for r in RUOLI_SALVATI {
        match portachiavi::carica(p, r, 1)? {
            Some(k) => pubbliche.push(k.pubblica_hex()),
            None => return Ok(None),
        }
    }
    Ok(Some(IdentitaPubblica { identity: pubbliche[0].clone(), dm: pubbliche[1].clone(), wallet_impronta: crate::impronta_wallet(&pubbliche[2]) }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portachiavi::Memoria;
    use crate::Ruolo;

    fn risposte_giuste(c: &CreazioneInCorso) -> Vec<String> {
        c.posizioni().iter().map(|&i| c.parole()[i].to_string()).collect()
    }

    #[test]
    fn tre_posizioni_diverse_e_casuali() {
        let mut viste = std::collections::HashSet::new();
        for _ in 0..30 {
            let c = CreazioneInCorso::nuova().unwrap();
            let p = c.posizioni();
            assert!(p[0] < p[1] && p[1] < p[2] && p[2] < 24, "{p:?}");
            viste.insert(p);
        }
        assert!(viste.len() > 20, "posizioni poco casuali: {} diverse su 30", viste.len());
    }

    #[test]
    fn conferma_sbagliata_non_salva_niente() {
        let c = CreazioneInCorso::nuova().unwrap();
        let p = Memoria::default();
        let mut r = risposte_giuste(&c);
        r[1] = if r[1] == "zoo" { "abandon".into() } else { "zoo".into() };
        assert_eq!(c.conferma(&r, &p).unwrap_err(), IdentityError::ConfermaSbagliata);
        assert_eq!(c.conferma(&r[..2], &p).unwrap_err(), IdentityError::ConfermaSbagliata);
        assert!(p.voci.lock().unwrap().is_empty(), "salvato qualcosa con una conferma sbagliata");
    }

    #[test]
    fn conferma_giusta_salva_solo_tre_ruoli() {
        let c = CreazioneInCorso::nuova().unwrap();
        let p = Memoria::default();
        // maiuscole e spazi tollerati
        let r: Vec<String> = risposte_giuste(&c).iter().map(|w| format!(" {} ", w.to_uppercase())).collect();
        let id = c.conferma(&r, &p).unwrap();
        let voci = p.voci.lock().unwrap();
        let mut nomi: Vec<_> = voci.keys().cloned().collect();
        nomi.sort();
        assert_eq!(nomi, vec!["dm-v1", "identity-v1", "wallet-v1"], "voci salvate: {nomi:?}");
        // nel portachiavi non ci sono né il seme né le parole né la revoca
        let seme = seme_da_parole(&c.parole().join(" "), "").unwrap();
        let revoca = deriva(&seme, Ruolo::Revoke, 1).unwrap();
        for s in voci.values() {
            assert_ne!(s, revoca.segreto_per_portachiavi());
        }
        assert_eq!(id.identity.len(), 64);
        drop(voci);
        assert_eq!(identita_salvata(&p).unwrap(), Some(id));
    }

    #[test]
    fn recupero_salva_le_stesse_chiavi() {
        let c = CreazioneInCorso::nuova().unwrap();
        let a = Memoria::default();
        let id = c.conferma(&risposte_giuste(&c), &a).unwrap();
        let b = Memoria::default();
        let id2 = recupera(&c.parole().join("  "), &b).unwrap();
        assert_eq!(id, id2);
        assert_eq!(*a.voci.lock().unwrap(), *b.voci.lock().unwrap());
        assert!(recupera("ciao mondo", &b).is_err());
    }
}
