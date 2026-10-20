//! SOLO PER LE PROVE (tests/unit/protocol-vectors.test.js): stampa un post firmato nel lato Rust con il
//! seme PUBBLICO di prova, perché il JavaScript (packages/protocol) lo verifichi: stesso contratto nei
//! due sensi. Argomento: il testo del post.

use m4tr1x_identity::{deriva, seme_da_parole, Ruolo};
use m4tr1x_protocol::{firma_evento, Rete};

fn main() {
    let testo = std::env::args().nth(1).unwrap_or_else(|| "ciao".into());
    let seme = seme_da_parole(&(["abandon"; 23].join(" ") + " art"), "").unwrap();
    let k = deriva(&seme, Ruolo::Identity, 1).unwrap();
    let e = firma_evento(1, &testo, &[vec!["t".into(), "città".into()]], 1_760_000_123, Rete::Prova, &k.pubblica_hex(), |d| k.firma_schnorr(d)).unwrap();
    println!("{}", serde_json::to_string(&e).unwrap());
}
