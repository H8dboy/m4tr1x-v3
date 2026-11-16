# m4tr1x-celle — immagini a celle esagonali (foto; seconda versione, 12/11/2026 pomeriggio)

Idea del fondatore (vedi `docs/design/VIDEO_A_CELLE.md`): griglia a celle esagonali con il numero di celle che segue
la complessità, colore e dettaglio separati per cella, strati che arrivano uno dopo l'altro. Regola: aritmetica
semplice che qualsiasi dispositivo sa fare, niente formati complicati, nessuna dipendenza. Qui ci sono solo somme,
differenze, spostamenti di bit, qualche moltiplicazione e divisione intera.

## Cosa c'è

- **Griglia** (`lib.rs`): centri su un reticolo triangolare (celle esagonali), raggio dalla complessità (`raggio_per`,
  30–48 px), pixel → centro più vicino, **raffinamento 1→7** delle celle con un bordo netto dentro (scarto da
  colore + pendenza sopra la soglia, 20). Il decodificatore ricostruisce la griglia da raggio e bit delle celle
  raffinate: la geometria non viaggia.
- **Strato 0 colore** (Y, Cb, Cr medi per cella) e **strato 1 pendenza** (piano ai minimi quadrati di luce e colore,
  sei numeri per cella, misurati dal baricentro dei pixel). Dal pomeriggio del 12/11 i due strati sono **compressi**
  (`strati.rs`): ogni cella è prevista dalla media di quella a sinistra e di quella sopra (le sei celle di una cella
  raffinata dal loro centro) e il resto è scritto con la codifica aritmetica. 2,5–3 volte meno byte dei byte crudi.
- **Strato 2 dettaglio** (`dettaglio.rs`), riscritto nel pomeriggio del 12/11:
  - la luce (intera) e i colori (dimezzati 2×2) dell'immagine **vera** passano per una **trasformata a onde intera 5/3**
    (`onde.rs`: quella reversibile di JPEG 2000, due passi di sollevamento, 3 livelli);
  - la **media grossolana** (banda LL) è prevista dall'immagine ricostruita dagli strati 0 e 1: lo strato 2 porta solo
    la differenza, quasi niente; le bande alte sono quelle dell'immagine vera (prevederle dalla base costava +30 %:
    ai bordi delle celle la base ha gradini che l'immagine vera non ha);
  - quantizzazione con zona morta e passo per banda (le bande grossolane pesano di più sull'immagine, quindi passo
    più fino; la diagonale più largo), ricostruzione a 3/8 del passo sotto la metà;
  - **codifica aritmetica binaria adattiva** (`aritmetica.rs`: range coder alla LZMA, probabilità a 15 bit con due
    finestre di adattamento, corta e lunga) con contesti da orientamento, livello, quanto c'è intorno tra i
    coefficienti già scritti e intorno al coefficiente "genitore" del livello sopra;
  - scritto **a pezzi, cella per cella**: prima le medie, poi per ogni livello di finezza le celle in ordine di
    quanto dettaglio hanno mostrato al livello prima, in 8 gruppi (`GRUPPI`), ognuno con la sua lunghezza davanti.
    L'ordine delle celle lo ricava anche chi legge, senza byte in più. Uno strato troncato dà i gruppi interi che ci
    sono; il resto resta "come previsto". Ogni pezzo può essere un blob.
- Decodifica per strati (`Strati::Colore`, `ColoreEPendenza`, `Tutti`), byte rotti → errore o immagine sbagliata, mai
  panico. 18 prove (trasformata esatta anche con misure dispari, codifica aritmetica avanti/indietro, passo 1 quasi
  senza perdita, pezzi che migliorano uno dopo l'altro, troncature, byte a caso, strati 0/1 avanti e indietro).
- `examples/misura.rs`: legge un BMP a 24 bit, codifica a più passi (`-- passo,croma[,livelli,raggio,soglia] …`),
  confronta con i JPEG dello stesso file, stampa la progressione pezzo per pezzo. BMP e JPEG di prova si fanno con
  PowerShell (System.Drawing) in `%TMP%/m4-celle`.

## Misure (12/11/2026 pomeriggio, PC dell'officina, `--release`, parametri predefiniti)

PSNR su RGB. "Celle allo stesso PSNR" = byte di tutto (intestazione + 3 strati) interpolati tra i passi 32/16 … 4/2
al PSNR di quel JPEG; il JPEG è quello di System.Drawing (Huffman, 4:2:0; un mozjpeg starebbe un 10–15 % sotto).
Foto 1–3 sono foto di telefono ridotte a 1600 px.

| immagine | celle (raff.) | strati 0+1 | JPEG q50 | celle allo stesso PSNR | JPEG q75 | celle allo stesso PSNR | cod./dec. |
|---|---|---|---|---|---|---|---|
| render 1400×900 (fondo piatto) | 477 (32), r 45 | 1,6 KB @ 25,6 dB | 33,3 KB @ 43,5 dB | 9,8 KB (**0,29×**) | 40,2 KB @ 46,5 dB | 20,6 KB (**0,51×**) | 294/143 ms |
| foto di uno schermo 1200×1600 | 1837 (152), r 30 | 10,1 KB @ 23,8 dB | 110,0 KB @ 37,1 dB | 76,6 KB (**0,70×**) | 182,9 KB @ 39,5 dB | 130,8 KB (**0,71×**) | 461/223 ms |
| foto 1 (interno, 1600×1200) | 1466 (95), r 30 | 9,7 KB @ 25,4 dB | 128,0 KB @ 34,6 dB | 81,7 KB (**0,64×**) | 217,2 KB @ 36,2 dB | 144,4 KB (**0,66×**) | 451/214 ms |
| foto 3 (esterno, 1200×1600) | 1477 (92), r 30 | 8,7 KB @ 24,7 dB | 135,3 KB @ 35,6 dB | 94,6 KB (**0,70×**) | 226,5 KB @ 37,5 dB | 175,0 KB (**0,77×**) | 453/214 ms |
| foto 2 (rumorosa, 1200×1600) | 3871 (491), r 30 | 26,2 KB @ 19,2 dB | 308,6 KB @ 29,0 dB | 222,3 KB (**0,72×**) | 479,2 KB @ 31,9 dB | 332,8 KB (**0,69×**) | 510/273 ms |

**Progressione** (byte di tutto → PSNR, passo 32/16): strati 0+1, poi le medie, poi per ogni livello di finezza
il primo gruppo di celle (quelle con più dettaglio) e la fine del livello:

- render: 1,6 KB 25,6 dB → medie 3,0 KB 31,5 → liv. 3: 3,5 KB 33,5 … 4,1 KB 35,2 → liv. 2: 4,7 KB 37,2 … 5,3 KB 38,6 → liv. 1: 5,8 KB 39,8 … 6,3 KB 40,3
- foto di uno schermo: 10,1 KB 23,8 → 15,1 KB 25,9 → 17,5 KB 26,6 … 22,2 KB 28,5 → 27,5 KB 30,2 … 32,1 KB 31,9 → 34,8 KB 32,5 … 36,3 KB 32,8
- foto 1: 9,7 KB 25,4 → 17,1 KB 29,4 → 19,1 KB 30,0 … 23,0 KB 31,4 → 23,8 KB 31,5 … 25,2 KB 31,7 → 25,5 KB 31,8 … 25,9 KB 31,8
- foto 3: 8,7 KB 24,7 → 14,4 KB 28,6 → 16,3 KB 29,2 … 20,1 KB 30,6 → 22,6 KB 31,2 … 26,0 KB 31,7 → 27,1 KB 31,9 … 27,8 KB 32,0
- foto 2: 26,2 KB 19,2 → 36,7 KB 20,4 → 41,5 KB 20,7 … 59,1 KB 22,2 → 70,8 KB 22,8 … 108,4 KB 24,7 → 122,7 KB 25,4 … 154,7 KB 27,0

Si vede l'alveare al lavoro: sul render il primo ottavo delle celle di ogni livello porta quasi tutto il guadagno
(3,0 → 3,5 KB = +2 dB, i sette ottavi dopo +1,7 dB in tutto); sulle foto il primo gruppo vale sempre più dei
successivi. **Costo dell'ordine per cella** rispetto all'ordine di riga, misurato: +0,2–3,6 % di byte (contesti
meno continui ai bordi delle celle e 25 pezzi invece di 4). Vale il prezzo: è l'idea.

**Lettura onesta.** L'obiettivo era "metà dei byte del JPEG q50 a pari PSNR, o dirlo con i numeri": sul render è
molto sotto la metà (0,29), sulle foto siamo a **0,64–0,72**, cioè un terzo in meno del JPEG, non la metà. È quello
che fa un buon codificatore a onde con codifica aritmetica (JPEG 2000 sta lì): la parte "a celle" non toglie e non
aggiunge byte, aggiunge la **struttura**: un'anteprima intera in 1,6–10 KB (foto 2, rumorosa, 26 KB) e il dettaglio
che arriva a pezzi, cella per cella, dentro la stessa scheda. La mattina stessa lo strato 2 costava 8–10 volte il JPEG.

**Decisione presa misurando (12/11 pomeriggio): con la trasformata le celle devono essere grandi.** Su foto 2
(complessità 13) il raggio automatico di prima era 13 px: 24 144 celle, 170 KB solo di strati 0 e 1, e la qualità
finale identica a quella con raggio 30 (3 KB di strati 0 e 1) o 45. Le celle servono all'anteprima e alla media
grossolana; il dettaglio va da solo dove serve. Quindi: raggio automatico tra 30 e 48 (`RAGGIO_AUTO_MIN`), soglia di
raffinamento 20 (con 10: +5 % di byte in tutto per +0,5 dB nell'anteprima; con 255, mai raffinare: −5 % di byte,
−1,5 dB nell'anteprima). Il "numero di celle dalla complessità" resta: nel raggio e nel raffinamento zona per zona.

**Cose misurate e scartate il 12/11 pomeriggio** (banco di prova con variabili d'ambiente, poi tolto): prevedere le
bande alte dalla base (+30 % luce, +2 % colori); pesi uniformi delle bande (+20–35 %); 5 livelli invece di 3 (+10 %
sulla foto); zona morta più stretta e ricostruzione a metà (pari); una sola finestra di adattamento (ogni scelta
+3–15 %); probabilità a 11 bit (la finestra lunga non superava il 94 % e peggiorava tutto: con 15 bit è un
guadagno); contesti con più vicini (pari). Passo dei colori = metà di quello della luce: −3 % sulla foto, pari sul
render.

**Perché non c'è un passo per cella** ("più fine dove la cella è raffinata"): con la trasformata i coefficienti sono
zero dove non c'è dettaglio e costano una frazione di bit; i byte vanno da soli nelle celle ricche, senza deciderlo a
mano. Un passo più fino dove c'è già molto dettaglio sposterebbe byte dove l'occhio nota meno (mascheramento):
si può aggiungere dopo come scelta percettiva, non per il PSNR.

## Prossimo

1. Misura su telefono o mini PC: tempo e memoria (oggi i piani sono i32: ~23 MB per una foto da 2 MP; si può fare
   a strisce). Un confronto a occhio (non solo PSNR) su un paio di foto.
2. Strato 1: le pendenze costano ancora 3–4 byte per cella; provare una scala più grossa.
3. Reel: stessa griglia e stessa trasformata; per ogni fotogramma si prevede dal precedente **cella per cella**
   (spostamento intero per cella, poi il resto come qui). È il pezzo che decide se l'idea regge sul video.
4. Collegamento al trasporto: ogni strato (e ogni pezzo dello strato 2) un blob a pezzi (ADR-0005), scheda con gli
   hash; chi guarda scarica prima i pochi KB dell'anteprima, poi quello che la banda permette.
