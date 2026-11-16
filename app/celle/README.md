# m4tr1x-celle — immagini a celle esagonali (foto; seconda versione, 13/11/2026)

Idea del fondatore (vedi `docs/design/VIDEO_A_CELLE.md`): griglia a celle esagonali con il numero di celle che segue
la complessità, colore e dettaglio separati per cella, strati che arrivano uno dopo l'altro. Regola: aritmetica
semplice che qualsiasi dispositivo sa fare, niente formati complicati, nessuna dipendenza. Qui ci sono solo somme,
differenze, spostamenti di bit, qualche moltiplicazione e divisione intera.

## Cosa c'è

- **Griglia** (`lib.rs`): centri su un reticolo triangolare (celle esagonali), raggio dalla complessità (`raggio_per`),
  pixel → centro più vicino, **raffinamento 1→7** delle celle dove colore + pendenza non bastano. Il decodificatore
  ricostruisce la griglia da raggio e bit delle celle raffinate: la geometria non viaggia.
- **Strato 0 colore** (Y, Cb, Cr medi per cella) e **strato 1 pendenza** (piano ai minimi quadrati di luce e colore,
  sei numeri per cella, misurati dal baricentro dei pixel). Dal 13/11 i due strati sono **compressi** (`strati.rs`):
  ogni cella è prevista dalla media di quella a sinistra e di quella sopra (le sei celle di una cella raffinata dal
  loro centro) e il resto è scritto con la codifica aritmetica. 2,5–3 volte meno byte dei byte crudi.
- **Strato 2 dettaglio** (`dettaglio.rs`), riscritto il 13/11:
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
  - scritto **a pezzi** dalla banda più grossolana alla più fine, ogni pezzo con la sua lunghezza davanti: uno strato
    troncato dà le bande intere che ci sono; i pezzi dopo restano "come previsto". Ogni pezzo può essere un blob.
- Decodifica per strati (`Strati::Colore`, `ColoreEPendenza`, `Tutti`), byte rotti → errore o immagine sbagliata, mai
  panico. 18 prove (trasformata esatta anche con misure dispari, codifica aritmetica avanti/indietro, passo 1 quasi
  senza perdita, pezzi che migliorano uno dopo l'altro, troncature, byte a caso, strati 0/1 avanti e indietro).
- `examples/misura.rs`: legge un BMP a 24 bit, codifica a più passi, confronta con i JPEG dello stesso file, stampa
  la progressione pezzo per pezzo. BMP e JPEG di prova si fanno con PowerShell (System.Drawing) in `%TMP%/m4-celle`.

## Misure (13/11/2026, PC dell'officina, `--release`; PSNR su RGB, "luce" = solo Y)

Render 1400×900 (fondo piatto, complessità 0, raggio 45, 573 celle di cui 48 raffinate):

| codifica (passo luce/colori) | colore+pend. | dettaglio | totale | PSNR | luce | tempo cod./dec. |
|---|---|---|---|---|---|---|
| celle 24/12 | 1 863 | 5 704 | **7 619** | 41,8 dB | 42,6 | 475 / 196 ms |
| celle 16/8 | 1 863 | 7 848 | **9 763** | 43,6 dB | 44,8 | |
| celle 8/4 | 1 863 | 13 781 | **15 696** | 45,6 dB | 47,2 | |
| celle 4/2 | 1 863 | 27 307 | **29 222** | 47,2 dB | 49,5 | |
| JPEG q50 | | | 33 298 | 43,5 dB | 45,1 | |
| JPEG q85 | | | 46 500 | 47,4 dB | 49,6 | |

Foto di un monitor 1200×1600 (complessità 3, raggio 30, 2 749 celle di cui 304 raffinate):

| codifica | colore+pend. | dettaglio | totale | PSNR | luce | tempo cod./dec. |
|---|---|---|---|---|---|---|
| celle 24/12 | 15 479 | 34 020 | **49 631** | 34,5 dB | 35,0 | 771 / 332 ms |
| celle 16/8 | 15 479 | 53 165 | **68 776** | 36,5 dB | 37,3 | |
| celle 12/6 | 15 479 | 71 826 | **87 437** | 37,8 dB | 38,6 | |
| celle 8/4 | 15 479 | 111 561 | **127 172** | 39,5 dB | 40,3 | |
| celle 6/3 | 15 479 | 162 396 | **178 007** | 40,8 dB | 41,8 | |
| celle 4/2 | 15 479 | 260 868 | **276 479** | 42,8 dB | 43,7 | |
| JPEG q30 | | | 79 587 | 35,1 dB | 36,2 | |
| JPEG q50 | | | 109 972 | 37,1 dB | 38,0 | |
| JPEG q75 | | | 182 947 | 39,5 dB | 40,4 | |
| JPEG q85 | | | 286 509 | 41,4 dB | 42,3 | |

**A pari PSNR, byte delle celle / byte del JPEG:** render 0,29 (43,5 dB) e 0,63 (47,2 dB); foto 0,70 (37,1 dB),
0,69 (39,5 dB), 0,71 (41,4 dB). Ieri lo strato 2 costava 8–10 volte il JPEG.

**Progressione** (foto, passo 24/12): strati 0+1 = 15 KB a 24,3 dB (immagine intera, riconoscibile); + pezzo 0 (medie)
21 KB → 26,0 dB; + pezzo 1 30 KB → 28,8; + pezzo 2 42 KB → 32,8; + pezzo 3 50 KB → 34,5. Sul render: 1,9 KB → 3,4 →
4,7 → 6,3 → 7,6 KB per 26 → 32 → 36 → 39 → 42 dB.

**Lettura onesta.** L'obiettivo era "metà dei byte del JPEG q50 a pari PSNR, o dirlo con i numeri": sul render è
molto sotto la metà (0,29), sulla foto siamo a 0,70. La foto è una foto di uno schermo (trama fitta): è il caso
difficile; va misurato su foto normali (cielo, volti, strade) e su un telefono. Il PSNR è su RGB e il JPEG di
System.Drawing è un JPEG qualsiasi (Huffman, 4:2:0): un JPEG ottimizzato (mozjpeg) starebbe un 10–15 % sotto. Il
dettaglio arriva comunque **a pezzi** dentro la stessa struttura a celle: niente di tutto questo esiste in un JPEG.

**Cose misurate e scartate il 13/11** (banco di prova con variabili d'ambiente, poi tolto): prevedere le bande alte
dalla base (+30 % luce, +2 % colori); pesi uniformi delle bande (+20–35 %); 5 livelli invece di 3 (+10 % sulla
foto); zona morta più stretta e ricostruzione a metà (pari); una sola finestra di adattamento (ogni scelta +3–15 %);
probabilità a 11 bit (la finestra lunga non superava il 94 % e peggiorava tutto: con 15 bit è un guadagno); contesti
con più vicini (pari). Passo dei colori = metà di quello della luce: −3 % sulla foto, pari sul render.

**Perché non c'è un passo per cella** ("più fine dove la cella è raffinata"): con la trasformata i coefficienti sono
zero dove non c'è dettaglio e costano una frazione di bit; i byte vanno da soli nelle celle ricche, senza deciderlo a
mano. Un passo più fino dove c'è già molto dettaglio sposterebbe byte dove l'occhio nota meno (mascheramento):
si può aggiungere dopo come scelta percettiva, non per il PSNR.

## Prossimo

1. Foto normali (3–4 scatti di telefono) e una misura su telefono/mini PC: tempo e memoria (oggi i piani sono i32:
   ~23 MB per una foto da 2 MP; si può fare a strisce).
2. Strato 1: le pendenze costano ancora 4 byte per cella (6 numeri); provare una scala più grossa per le celle grandi.
3. Reel: stessa griglia e stessa trasformata; per ogni fotogramma si prevede dal precedente **cella per cella**
   (spostamento intero per cella, poi il resto come qui). È il pezzo che decide se l'idea regge sul video.
4. Collegamento al trasporto: ogni strato (e ogni pezzo dello strato 2) un blob a pezzi (ADR-0005), scheda con gli
   hash; chi guarda scarica prima i 2 KB, poi quello che la banda permette.
