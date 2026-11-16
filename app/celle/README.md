# m4tr1x-celle — immagini a celle esagonali (prima versione, foto)

Idea del fondatore (vedi `docs/design/VIDEO_A_CELLE.md`): griglia a celle esagonali con il numero di celle che segue
la complessità, colore e dettaglio separati per cella, strati che arrivano uno dopo l'altro. Regola: aritmetica
semplice che qualsiasi dispositivo sa fare, niente formati complicati, nessuna dipendenza.

## Cosa c'è (12/11/2026)

- **Griglia** triangolare di centri (celle esagonali), raggio dalla complessità (`raggio_per`), pixel → centro più
  vicino, **raffinamento 1→7** delle celle dove colore + pendenza non bastano. Il decodificatore ricostruisce la
  griglia da raggio e bit delle celle raffinate: la geometria non viaggia.
- **Strato 0 colore** (Y, Cb, Cr medi per cella, 3 byte), **strato 1 pendenza** (6 byte per cella: piano ai minimi
  quadrati di luce e colore, misurato dal baricentro dei pixel della cella), **strato 2 dettaglio** (resto della luce
  pixel per pixel, quantizzato con zona morta, zeri contati, numeri a lunghezza variabile).
- Decodifica per strati (`Strati::Colore`, `ColoreEPendenza`, `Tutti`): uno strato mancante o troncato dà
  un'immagine intera, meno fine. Byte rotti → errore, mai panico.
- Prove: colore avanti/indietro, raggio dalla complessità, celle compatte e raffinamento, numeri variabili, gradiente
  (nessuna cella raffinata, 44,9 dB con colore + pendenza e 3 byte di dettaglio), scacchiera (raffina i bordi), byte
  rotti, PSNR.
- `examples/misura.rs`: legge un BMP a 24 bit, codifica a più passi, confronta con i JPEG dello stesso file.

## Prime misure (12/11/2026, PC dell'officina, `--release`)

Render 1400×900 (fondo piatto, complessità ~0, raggio 45, 573 celle di cui 48 raffinate):

| codifica | byte | PSNR | tempo cod./dec. |
|---|---|---|---|
| celle: colore + pendenza | 5 207 | 26,1 dB | 330 / 160 ms |
| celle + dettaglio passo 16 | 207 562 | 40,5 dB | |
| celle + dettaglio passo 8 | 279 076 | 43,7 dB | |
| JPEG q30 | 28 602 | 38,9 dB | |
| JPEG q50 | 33 298 | 43,5 dB | |
| JPEG q85 | 46 500 | 47,4 dB | |

Foto di un monitor 1200×1600 (complessità 3, raggio 30, 2 749 celle di cui 304 raffinate):

| codifica | byte | PSNR | tempo cod./dec. |
|---|---|---|---|
| celle: colore + pendenza | 24 871 | 24,3 dB | 480 / 260 ms |
| celle + dettaglio passo 16 | 734 275 | 34,7 dB | |
| celle + dettaglio passo 8 | 1 319 521 | 37,8 dB | |
| JPEG q30 | 79 587 | 35,1 dB | |
| JPEG q50 | 109 972 | 37,1 dB | |
| JPEG q85 | 286 509 | 41,4 dB | |

**Lettura onesta.** Gli strati 0 e 1 fanno quello che l'idea promette: in 5–25 KB l'immagine arriva intera e
riconoscibile, ed è la parte che un nodo su una SIM può servire sempre. Lo strato 2 di oggi è ingenuo (resto pixel
per pixel) e costa **8–10 volte** il JPEG a pari qualità: è lì che vanno adattati i "trent'anni di trucchi" (trasformata
del resto per blocchi o a onde, quantizzazione che segue la cella, codifica entropica adattiva), restando aritmetica
intera. Difetti trovati dalle prove: raffinamento deciso sullo scarto dalla media (un gradiente liscio veniva
raffinato) → ora sullo scarto da colore + pendenza; pendenze misurate dal centro geometrico (celle tagliate dal bordo
avevano piani storti) → ora dal baricentro dei pixel; prodotti di somme in 64 bit → 128 bit.

## Prossimo

1. Strato 2 con una trasformata del resto (Haar a due livelli sull'immagine del resto, poi quantizzazione con zona
   morta e codifica entropica adattiva): obiettivo arrivare alla metà dei byte del JPEG q50 a pari PSNR, o dichiarare
   il contrario con i numeri.
2. Passo di quantizzazione per cella (più fine dove la cella è raffinata).
3. Reel: stessa griglia, predizione dal fotogramma prima cella per cella.
4. Collegamento al trasporto a pezzi: ogni strato un blob, scheda con gli hash degli strati.
