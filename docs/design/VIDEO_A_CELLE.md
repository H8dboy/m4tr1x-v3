# Video a celle esagonali (idea del fondatore, ripresa il 12/11/2026)

> **Priorità (fondatore, 12/11/2026): questo viene prima del resto del social.** "Se vinciamo sulla banda abbiamo vinto su tutto: a quel punto M4TR1X è realtà, il resto può aspettare." Codice: `app/celle` (PR #14), misure nel suo README.
>
> Stato: prima versione per le foto scritta e misurata il 12/11 (strati base 5–25 KB per un'immagine intera; dettaglio 8–10× il JPEG). Nel pomeriggio del 12/11 lo strato del dettaglio è stato rifatto (trasformata a onde intera 5/3, codifica aritmetica adattiva, pezzi progressivi) e gli strati base compressi: a pari PSNR le celle stanno a 0,29× il JPEG q50 su un render e a 0,64–0,72× su quattro foto, con il dettaglio che arriva a pezzi **cella per cella** (prima le celle con più dettaglio, ordine ricavato da chi legge senza byte in più; costa +0,2–3,6 %; misure nel README del crate). Scritta qui perché una prima versione era andata persa. Prossimo: foto normali e telefono, poi i reel (previsione cella per cella dal fotogramma prima). Non cambia il trasporto a pezzi (ADR-0005), lo completa.

## Il problema che risolve

I mini PC sparsi (nodi sulle macchine Elea, su una SIM) non devono spendere banda né calcolo per i video. Oggi il
piano dice: conversione solo sugli hub, nodi ed edge fanno cache. Resta il secondo problema: su una rete lenta
(Tor, SIM) un video a flusso continuo cala di qualità su tutto il fotogramma, o si ferma.

## L'idea

1. **All'upload** il video viene diviso in una **griglia a celle esagonali** (alveare), non in pixel né in riquadri
   quadrati. Ogni cella è un'unità con la sua storia nel tempo.
   - **La griglia è adattiva:** il numero di celle si calcola dalla **complessità** del fotogramma (o dell'immagine):
     una zona piatta (cielo, muro) ha poche celle grandi, una zona ricca (volto, scritta, trama) ne ha tante e piccole.
   - **Dentro ogni cella colore e dettaglio sono separati:** il colore è poca informazione e arriva subito pieno; il
     dettaglio (struttura, bordi, trama) è la parte che si affina dopo. Così con poca banda non si "sgrana" come con i
     pixel: non si buttano via punti, si rimanda il dettaglio e il colore resta intero.
2. Si manda **prima una versione grezza di tutte le celle**: poca banda, il video si vede subito, intero.
3. **Ogni cella si ricalcola e si affina dopo**, quando la banda c'è, per prima quelle che cambiano di più o che si
   guardano di più. Il raffinamento è a pezzi piccoli e indirizzati dall'hash come tutto il resto (cache sugli edge).
4. Una rete lenta **abbassa la finezza di qualche cella**, non fa crollare il fotogramma; nessuno in mezzo
   ricodifica niente.

## Perché l'esagono

Sei vicini tutti alla stessa distanza: i bordi tra una cella e l'altra si notano meno che con i quadrati (niente
"blocchetti"), e il raffinamento si distribuisce in modo più uniforme.

## Parentele (da cui si può partire senza inventare tutto)

- Codifica a riquadri (tiles) dei codec moderni (HEVC/AV1): il fotogramma in parti decodificabili da sole.
- Raffinamento progressivo e codifica scalabile (SVC, LCEVC): uno strato base più strati di miglioramento.
- Codifica guidata dall'attenzione (foveated): finezza dove si guarda.

Nessuno di questi usa l'esagono: quella parte è da costruire e misurare.

## Dove si incastra nel piano v3

- **Trasporto:** invariato, pezzi da 4 MB come blob (ADR-0005). Lo strato grezzo e ogni raffinamento sono blob.
- **Hub:** è lì che si fa la griglia e il calcolo (ffmpeg o altro, I-27). I nodi piccoli non toccano nulla.
- **Edge:** cache dello strato grezzo di tutto e dei raffinamenti più richiesti.
- **App:** mostra lo strato grezzo appena arriva e sostituisce le celle man mano che arrivano le versioni fini.

## Conseguenza (fondatore, 12/11/2026): può togliere l'hub convertitore

Se la griglia si calcola **sul dispositivo di chi carica**, al momento dell'upload, in rete non serve più una macchina
che riceve l'originale, lo converte e firma il manifest: spariscono il ruolo `hub` come convertitore, la chiave K8 e
il rischio "due hub caduti = video nuovi fermi". Il manifest delle celle lo firma l'autore, come la scheda di oggi.
Resta solo **un nodo con disco e banda** che tiene i blob: non è un ruolo speciale, è il "nodo dei file" già
presente nell'app (12/11). Niente calcolo, niente fiducia in più.

**Il rischio da non sottovalutare:** è un formato video nuovo; nessun telefono o browser lo codifica o decodifica da
solo, e un codec da zero è il lavoro più duro del campo. **Strada che tiene l'idea e schiva il rischio:** le celle sì,
il codec no. Ogni cella si codifica con un formato che l'hardware sa già leggere (es. AV1/HEVC a più livelli di
finezza); griglia adattiva, separazione colore/dettaglio e raffinamento progressivo stanno nel modo in cui si spezza,
si spedisce e si ricompone. Il telefono usa il suo decoder; il risultato per la rete è lo stesso.

## Scelta di metodo (fondatore, 12/11/2026): aritmetica semplice, niente formati nuovi e complicati

Tornare alla base: un fotogramma è una matrice di numeri; medie, differenze e somme per cella le sa fare qualsiasi
dispositivo, in poche centinaia di righe senza dipendenze (come `app/media` per la pulizia). Niente scatole nere,
risultato verificabile ovunque allo stesso modo. Il confine è il **costo al secondo**: per foto, reel corti e anteprime
basta; per un film lungo in alta definizione il calcolo sul processore scalda e consuma più del chip dedicato, e i
formati esistenti stringono di più. Perciò, in ordine:

1. **Foto e reel prima:** celle adattive, colore medio per cella, strati di dettaglio come differenze, aritmetica
   intera, nel crate dei media, con prove su file veri.
2. **Misurare:** byte per immagine rispetto al JPEG, tempo di calcolo su PC e telefono, resa a banda bassa.
3. **Film dopo, con i numeri in mano:** se regge, bene; se no, le celle restano e dentro ci va il decoder del dispositivo.

## Domande aperte prima di progettarla

- La misura di complessità che decide il numero di celle (energia dei bordi? varianza locale? movimento tra fotogrammi?) e quanti livelli di finezza per cella.
- Come si separano colore e dettaglio dentro la cella (luminanza/crominanza come nei codec, o una descrizione del colore per zone più una del dettaglio).
- Come si descrive una cella in un manifest firmato dall'hub (K8) e come si verifica (I-17 per ogni blob).
- Quanto costa all'hub rispetto all'HLS a più qualità, che è la strada già decisa e più semplice da fare prima.
- Misure su Tor: banda reale per cella, ritardo fino al primo fotogramma intero.
