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

## Reel (13/11/2026, `examples/misura_reel.rs`, due video del telefono di 60 fotogrammi)

Stessa griglia e stessa trasformata della foto. Il primo fotogramma è una chiave; gli altri si prevedono **cella
per cella** da fotogrammi già ricostruiti: uno spostamento `(dx, dy)` per cella a mezzo pixel (quarto sui colori
dimezzati), bordi tra celle sfumati (media pesata, come l'OBMC di H.263), il resto a onde con la base intera
(`Base::Tutto`), contesti aritmetici tenuti da un fotogramma all'altro, un bit "cella vuota" per livello. Il resto
arriva cella per cella, prima le celle dove la previsione ha sbagliato di più.

**Bidirezionali a piramide** (come i B di x264): tra un'àncora e l'altra (ogni 4 fotogrammi) il mezzo si prevede
dall'àncora prima e da quella dopo, i quarti dal mezzo e da un'àncora; ogni cella sceglie passato, futuro o media.
Misurato: la doppia previsione da sola rende il 2 % (i video del telefono sono rumorosi: a passo fino si codifica
rumore, e la media di due riferimenti lo abbassa poco). **Il guadagno sta nel passo più grosso in fondo alla
piramide** (+3/8 per livello: il mezzo ×1,375, i quarti ×1,75): le foglie non fanno da riferimento e tra due
fotogrammi buoni l'occhio non le nota.

| video | catena di previsti (bidir 0) | piramide (bidir 3) | pari PSNR | piramide lunga (bidir 7) |
|---|---|---|---|---|
| reel1 428×640 (passeggiata, rumoroso) | passo 12: 5542 B/fotogr. 36,64 dB · 14: 4351 B 35,42 dB | passo 12: 3835 B 36,20 dB | **−24 %** | 3208 B 35,70 dB (−30 %) |
| reel2 640×360 (fermo, poco rumore) | passo 16: 1444 B 37,50 dB · 20: 1082 B 36,27 dB | passo 16: 1029 B 37,46 dB | **−28 %** | 901 B 37,20 dB (−33 %) |

**Celle che seguono il moto** (13/11 pomeriggio, idea del fondatore: "celle intelligenti che predicono la dimensione in
base a quando il moto cambia"): la griglia non è più quella della chiave. Ogni cella base può essere intera o divisa
in sette, fotogramma per fotogramma. Chi legge **prevede da solo** la griglia: una cella divisa resta divisa finché
dentro i sette spostamenti non sono uniformi, poi torna intera (zero byte). Le divisioni nuove le dice chi scrive
con un bit adattivo per cella base (nessun byte se non cambia niente): prova a dividere le celle con scarto medio per
pixel sopra passo/8 e tiene la divisione se lo scarto cala di più di 12·passo; riunisce quelle che non pagano più.
Misurato a pari passo (bidir 3): reel1 passo 12 → 3730 B/fotogramma e 36,28 dB contro 3868 B e 36,08 dB (−3,6 %,
+0,2 dB); reel2 passo 16 → 1010 B e 37,60 dB contro 1024 B e 37,31 dB (−1,4 %, +0,3 dB). A pari PSNR vale circa
**−7/−8 %**. Celle per fotogramma: 423 contro 160 su reel1, 189 contro 96 su reel2; i bit di correzione costano 7–14 B
a fotogramma. Lezioni misurate: prevedere la divisione quando due vicine intere si muovono diversamente sbaglia (il
confine passa TRA le celle, non dentro) e costa un bit a ogni errore; con la soglia a 1·passo un video con la camera
ferma non divideva mai (la media su una cella grande resta bassa anche con un soggetto che si muove); partire dalla
griglia della chiave invece che da celle intere non cambia niente. Codifica ~0,6 s a fotogramma (fino a tre ricerche),
decodifica ~60 ms.

Confronto con i codec veri sugli stessi fotogrammi (ffmpeg, `baseline-x264.txt` nelle cartelle di prova): x264 medium
crf 24 fa 2485 B a 36,16 dB su reel1 e 1067 B a 37,96 dB su reel2; a pari PSNR le celle stanno a **~1,5× x264 su
reel1 e ~1,15× su reel2** (bidir 7), contro le 8–10× del codec ingenuo di due giorni fa. MJPEG q50 è a 26.000 B su
reel1. Gli spostamenti costano 90–190 B per fotogramma. Tempo: ~250 ms per fotogramma a codificare (ricerca su due
riferimenti), ~40 ms a decodificare, PC dell'officina senza ottimizzazioni.

Comando: `cargo run --release --example misura_reel -- <cartella con frame-NNN.bmp> -- passo,croma,chiave_ogni,cerca,raggio,bidir …`.

## Provato sul nodo via Tor (16/11/2026)

`cargo run --release -p m4tr1x-feed --example celle -- <onion> <dati> <foto.bmp>` contro il nodo di prova: foto 1600×1200,
4 pezzi, 177.562 B; carico 26 s; scheda a celle pubblicata e riletta; riscarico pezzo per pezzo: anteprima 9.661 B
dopo 1 s (PSNR 25,4 dB), 33,6 dB a 6 s, 35,9 dB a 10 s, completa a 14 s (36,8 dB). Pezzi identici.
Reel (stesso esempio con la cartella dei fotogrammi): 24 fotogrammi 428×640, chiave ogni 12 → 2 pezzi, 207.292 B; carico
30 s; il primo gruppo di 12 fotogrammi si guarda dopo 9 s, tutto dopo 22 s (38,6 dB). Via Tor ~10 KB/s: il collo è la rete.

## Pacchetto e app (16/11/2026): l'alveare arriva nel social

`pacchetto.rs` è il formato di viaggio (ADR-0005: ogni pezzo un blob con il suo hash):

- **foto:** pezzo 0 = `M4CF` + intestazione + strato 0 + strato 1 (l'anteprima: l'immagine intera in pochi KB),
  poi un pezzo per livello di finezza dello strato 2 (le medie con il più grossolano). I pezzi del dettaglio sono un
  prefisso dello strato intero: con quelli che ci sono si decodifica, il resto resta "come previsto";
- **reel:** un pezzo per gruppo di fotogrammi da una chiave alla successiva, ognuno un reel completo (`M4RL`) che si
  legge da solo; `fps` è scritto nel formato (VERSIONE_REEL 4). Per renderlo possibile l'encoder codifica i
  fotogrammi tra l'ultima àncora e una chiave programmata in catena PRIMA della chiave (fino al 16/11 una chiave
  programmata con fotogrammi in mezzo faceva ricodificare TUTTO il reel senza bidirezionali) e la chiave cade davvero
  ogni `chiave_ogni` fotogrammi (prima solo sulle àncore multiple di 4). Misurato su reel1: una chiave ogni 30 costa
  +7 % rispetto a una chiave sola (239.698 contro 223.824 byte), contro il +54 % della catena di prima.

Nell'app (`app/shell`, comandi `celle_*`; `app/protocol`: tipi `image/vnd.m4tr1x.celle` e `video/vnd.m4tr1x.celle`,
scheda con `["pezzo", hash, byte]` per ogni pezzo, `strati_validi`; `app/feed`: `carica_pezzi`, `scarica_pezzi` con
un richiamo a ogni pezzo arrivato): l'interfaccia legge i pixel del file scelto con il decodificatore del browser
(foto ridotta a 1600 px di lato; video a 480 px, 12 fotogrammi al secondo, 30 s al massimo) e li manda grezzi al
lato Rust, che codifica (`celle_prepara`), pubblica i pezzi con un'autorizzazione firmata per pezzo e la scheda
firmata (`celle_pubblica`), e all'apertura scarica i pezzi uno per uno via Tor decodificando man mano in un filo a
parte (`celle_apri`, `celle_stato`, `celle_quadro`): chi guarda vede la foto intera dal primo pezzo e la vede
affinarsi; un reel parte dal primo gruppo. Nessun hub ricodifica niente, nessun metadato da ripulire (il formato
porta solo pixel), e al JavaScript non arriva mai un file: solo pixel.

## Prossimo

1. Misura su telefono o mini PC: tempo e memoria (oggi i piani sono i32: ~23 MB per una foto da 2 MP; si può fare
   a strisce). Un confronto a occhio (non solo PSNR) su un paio di foto.
2. Strato 1: le pendenze costano ancora 3–4 byte per cella; provare una scala più grossa.
3. Reel: siamo a ~1,1–1,4× x264 medium a pari PSNR (vedi sopra). Da provare: un secondo livello di divisione (7×7:
   con il raggio automatico a 48 le celle divise sono ancora da 18 px), pesi diversi da ½ nella media dei due
   riferimenti (dissolvenze), un passo più grosso sulle celle che l'occhio segue meno, meno ricerche per chi scrive.
4. Collegamento al trasporto: FATTO il 16/11 (`pacchetto.rs`, comandi `celle_*`). Da provare sul nodo Oracle via Tor
   con una foto e un reel veri; poi: più fotogrammi al secondo e più lato per i reel (oggi la codifica costa ~0,3 s a
   fotogramma a 480 px), audio nei reel (oggi solo immagini), le schede a celle anche tra i Film.
