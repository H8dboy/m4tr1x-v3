//! L'UNICO tipo che porta le 24 parole verso un JavaScript: solo la finestra del seme
//! (capability "seme", controllo dell'etichetta nel comando), solo per mostrarle (I-58).
//! I-31 vieta questo tipo in qualsiasi altro comando.

use serde::Serialize;

#[derive(Serialize)]
pub struct SemeDaMostrare {
    pub parole: Vec<String>,
    /// posizioni (da 1) che l'utente dovrà riscrivere: non sono un segreto
    pub posizioni: Vec<u8>,
}

impl std::fmt::Debug for SemeDaMostrare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SemeDaMostrare(…)")
    }
}
