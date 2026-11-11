//! SOLO PROVE: pulisce un file vero e scrive la copia pulita altrove (l'originale non si tocca).
//! Uso: cargo run --example ripulisci -p m4tr1x-media -- <file> <copia pulita>
fn main() {
    let mut a = std::env::args().skip(1);
    let (da, a_) = (a.next().expect("file"), a.next().expect("copia pulita"));
    let b = std::fs::read(&da).expect("lettura");
    match m4tr1x_media::ripulisci(&b) {
        Ok((t, p)) => {
            std::fs::write(&a_, &p).expect("scrittura");
            println!("{t:?}: {} -> {} byte", b.len(), p.len());
        }
        Err(e) => println!("rifiutato: {e}"),
    }
}
