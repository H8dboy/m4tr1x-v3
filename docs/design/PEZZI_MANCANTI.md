# Pezzi mancanti — proposta di progetto (16/10/2026)

> **Stato: BOZZA da approvare.** Nessun codice. Ogni pezzo diventa codice solo con l'OK del fondatore, dentro la fase indicata, con le invarianti scritte prima.
> Numeri proposti (I-43…I-50, K16, K17; I-42 è già proposta per i topic in MIGRATION_PLAN) **non** sono ancora in SECURITY_INVARIANTS.md / KEYS.md: entrano lì all'approvazione.

Quattro pezzi chiesti dal fondatore il 15/10: monitoraggio, registrazione dei creator, shop fisico, boost. Per ognuno: cosa fa, come, perché così, il compromesso, cosa si potrebbe sbagliare.

---

## 1. Monitoraggio

**Cosa serve.** Sapere se un nodo sta bene (spazio, eventi, errori, Tor su o giù) e, per il fondo nodi (Q16), sapere quali nodi sono stati raggiungibili nel mese.

**Come.**
- **Sul nodo:** solo **contatori** (eventi accettati/rifiutati per motivo, blob, byte usati, circuiti Tor, errori, tempo di avvio). Niente IP, niente pubkey di chi si collega, niente onion dei client, niente id di eventi. Esposti **solo** sul listener owner (loopback) con K13, mai su quello pubblico.
- **Raggiungibilità:** le **ancore** provano ogni nodo della directory via onion a intervalli casuali (es. 6 prove al giorno, orari diversi) e firmano con K7 un riepilogo mensile "nodo X: N prove riuscite su M". È questo che usa il fondo nodi: nessuno dichiara il proprio uptime da solo.
- **Statistiche pubbliche (facoltative):** solo totali di rete arrotondati (es. "~40 nodi attivi"), pubblicati dalle ancore.

**Perché.** Il nodo non deve diventare un registro di chi lo usa: un sequestro del nodo non deve dare più di quello che è già pubblico.

**Compromesso.** Senza dati per utente non si può fare diagnosi fine ("perché a quell'utente non carica"): si diagnostica dal lato client, con i log dell'app che restano sul telefono e che l'utente può decidere di mandare.

**Invarianti proposte.**
- **I-43** — Le metriche del nodo non contengono IP, pubkey, onion o id di contenuti; rispondono solo su loopback con K13. Test: scansione della risposta delle metriche dopo traffico di prova con valori riconoscibili → nessuno presente; richiesta dal listener pubblico → 404.
- **I-44** — Il riepilogo di raggiungibilità usato per pagare i nodi è firmato da almeno 2 ancore su 3. Test: riepilogo con una sola firma → rifiutato dal calcolo del fondo.

**Fase:** contatori in **F2-bis** (piccola, il nodo esiste già); prove delle ancore in **F3-bis**; uso per il fondo in **F7**.

---

## 2. Registrazione dei creator

**Il problema.** Le mance vanno ai creator (Q1), ma per scelta i conti H8 **non** sono collegati alle identità Nostr in pubblico (ADR-0008, nessun id conto negli eventi). Serve un modo per sapere "la mancia per questa pubkey va su quel conto" senza scriverlo in giro.

**Come.**
1. Il creator, dall'app, prepara una **richiesta di registrazione** con due firme: una della chiave identità (Schnorr, "sono io questa pubkey") e una di K2 del wallet (ML-DSA, "questo conto è mio"), legate tra loro dallo stesso testo con dominio `creator/register` e la rete.
2. La richiesta è **cifrata per l'emittente** e inviata via Tor. Non passa dai relay pubblici.
3. L'emittente (repo privato `m4tr1x-issuer`) tiene una **mappa privata** pubkey → conto. Risponde con una ricevuta firmata K10.
4. **Revoca:** il creator manda `creator/revoke` (firmata con identità **o** con la chiave di revoca) → la mappa viene cancellata, le mance successive tornano indietro a chi le manda.
5. **Mancia:** l'app di chi dona firma con K2 una spesa verso "il creator con pubkey P", con P **cifrata per l'emittente**. L'emittente risolve P → conto e scrive nel registro pubblico una riga conto → conto con lo split (50/30/20, quota nodo al fondo nodi). Nel registro pubblico non compare P.

**Perché.** Chi guarda il registro vede soldi che si spostano tra conti, non chi ha dato una mancia a chi.

**Compromesso (da scrivere chiaro nell'app).** L'**emittente** sa quale conto appartiene a quale creator e quale conto ha mandato mance a quale creator. È il punto centrale dichiarato (contestazione 2 del 15/10). Chi vuole dare mance senza farsi collegare usa un conto ricaricato con buoni ciechi (F7b).

**La mappa è un bersaglio.** È l'unico posto dove identità e wallet stanno insieme: chi la ruba sa chi è dietro ogni conto di creator. Regole:
- **Minima:** per ogni creator solo due campi, `indice = HMAC-SHA256(K17, pubkey)` e `conto cifrato` (AES-GCM con K17). Niente data, niente IP, niente nome, niente note.
- **Cifrata:** senza K17 il database non dice nulla, nemmeno quali pubkey sono registrate (l'indice è un HMAC, non la pubkey). K17 sta fuori dal database, nel vault del server dell'emittente con passphrase (come K5/K13 sul nodo); i backup contengono solo righe cifrate.
- **Nessuno storico:** la revoca **cancella** la riga (non la segna come revocata). Una nuova registrazione sovrascrive quella vecchia.
- **Nessuna traccia delle mance:** durante una mancia l'emittente decifra P, cerca la riga, scrive la riga pubblica conto → conto e **butta P**. Niente log "il conto X ha cercato il creator P".
- **Ruotabile:** K17 nuova → tutte le righe ricifrate in un passaggio; la vecchia distrutta.

**Cosa resta scoperto (da dire chiaro).** Chi prende il server **mentre gira** (K17 in memoria) legge la mappa. E l'emittente in buona fede la legge sempre, per costruzione: è il punto centrale dichiarato.

**Invarianti proposte.**
- **I-45** — Una registrazione è accettata solo con entrambe le firme (identità e K2) sullo stesso testo e la stessa rete. Test: richiesta con firma K2 di un altro conto → rifiutata; testo cambiato dopo la firma → rifiutato.
- **I-46** — Né la pubkey del creator né l'id del conto compaiono negli eventi Nostr o nelle righe pubbliche delle mance. Test statico + dinamico sulla riga generata.

- **I-50** — La mappa creator contiene solo `HMAC(K17, pubkey)` e il conto cifrato; nessuna pubkey, nessun id conto in chiaro nel database, nei backup o nei log dell'emittente, nemmeno durante una mancia. Test: registrazione + mance + revoca con pubkey e conto riconoscibili → scansione di database, backup e log → nessuno dei due in chiaro; dopo la revoca → nessuna riga.
- K17 — chiave della mappa creator, nel vault del server dell'emittente; se rubata **insieme** al database: tutti i collegamenti creator ↔ conto.

**Fase:** F7 (insieme alle mance).

---

## 3. Shop fisico

**Cosa.** Vendere oggetti fisici di H8-Group pagando in H8. Lo shop **non** sta sui nodi: è un servizio di H8-Group, come l'emittente.

**Come.**
1. Il **catalogo** è pubblico: eventi firmati dalla chiave dello shop (prodotti, prezzo in H8, disponibilità).
2. L'**ordine** viene composto nell'app: prodotti + **indirizzo di spedizione**. Indirizzo, nome e telefono sono **cifrati sul dispositivo** per la chiave dello shop (**K16**, nuova) e inviati via Tor. Mai su un relay pubblico, mai in chiaro su un nodo.
3. Il **pagamento** è una spesa H8 firmata K2 che cita l'**hash dell'ordine** (non l'indirizzo). Lo shop vede che l'ordine è pagato.
4. **Conservazione minima:** l'indirizzo si cancella **30 giorni dopo la consegna** (tempo per un reso); restano solo i dati che la legge obbliga a tenere per la contabilità, separati dal resto.
5. **Stato dell'ordine:** messaggi cifrati shop → cliente (NIP-17), così non serve un account con email.

**Perché.** Un indirizzo di casa è il dato più pericoloso che il sistema toccherà: deve stare in un posto solo, per poco tempo, cifrato.

**Compromesso.** Chi compra un oggetto fisico **si fa conoscere dallo shop** (nome, indirizzo) e lo shop può collegare quell'ordine al conto che ha pagato. È inevitabile per una spedizione; va detto prima del pagamento. Chi non vuole il collegamento paga con un conto ricaricato da buoni ciechi.

**Da chiarire fuori dal codice (legale).** Vendita a distanza = partita IVA, fatture/scontrini, diritto di recesso di 14 giorni, prezzi anche in euro. Va nel parere legale insieme a Q2/Q12. Lo shop si accende solo dopo.

**Invarianti proposte.**
- **I-47** — L'indirizzo di spedizione esce dal dispositivo solo cifrato per K16, e lo shop lo cancella entro il termine. Test: ordine di prova con indirizzo riconoscibile → assente in chiaro da traffico, relay e log; dopo il termine simulato → assente dal database dello shop.
- K16 — chiave di cifratura dello shop, sul server dello shop, ruotabile; se rubata: gli indirizzi degli ordini **non ancora cancellati**.

**Fase:** nuova **F7c**, dopo le mance e dopo il parere legale.

---

## 4. Boost

**Cosa.** Pagare H8 a H8-Group per dare più visibilità a un post.

**Come.**
1. Il boost è una **spesa H8 nel registro** verso il conto della piattaforma, firmata K2, che cita l'**id dell'evento** (del post) e la durata. Chiunque può boostare qualsiasi post (anche non suo).
2. I **client** leggono la copia del registro (già replicata su nodi ed edge, F7) e calcolano per ogni post un punteggio di boost che **decade** nel tempo (es. dimezza ogni 24 h).
3. L'ordine del feed lo fa **l'app**, non un server: il boost può far salire un post **al massimo di un numero fisso di posizioni** e il post compare sempre con l'etichetta **"Promosso"**. Il feed di chi segui resta cronologico; il boost agisce solo nelle viste "scopri".
4. L'utente può spegnere i post promossi nelle impostazioni.

**Perché.** Senza server centrale nessuno "vende la posizione": le regole di ordinamento sono nel codice dell'app, pubbliche e uguali per tutti, e il registro dimostra chi ha pagato cosa.

**Compromesso.** Il registro è pubblico: si vede che **un certo conto** ha boostato **un certo post**. Se quel conto è collegabile a una persona (bonifico senza buoni ciechi), la persona è collegata al post. Va scritto nell'app; per boostare in modo non collegabile si usa un conto ricaricato con buoni ciechi.
Contraddizione da guardare: l'id dell'evento nel registro **non** è un id conto negli eventi (la regola di F1 resta rispettata), ma collega registro ↔ contenuto. È accettabile perché il collegamento è voluto e visibile; non va usato per nient'altro.

**Invarianti proposte.**
- **I-48** — Un post promosso è sempre etichettato e non sale di più di N posizioni. Test sull'algoritmo di ordinamento con boost enormi → limite rispettato.
- **I-49** — Il boost conta solo se la riga è nel registro firmato (K10) e non scaduta. Test: boost "dichiarato" in un evento Nostr senza riga → ignorato.

**Fase:** F7 (riga del registro) + F4/F9 (ordinamento nell'app).

---

## Riepilogo per il fondatore

| Pezzo | Cosa decidere | Raccomandazione |
|---|---|---|
| Monitoraggio | prove di raggiungibilità dalle ancore come base del fondo nodi | sì |
| Registrazione creator | l'emittente conosce creator ↔ conto (dichiarato) | sì, con revoca e mappa cifrata |
| Shop fisico | servizio separato di H8-Group, indirizzi cancellati dopo 30 gg | sì, acceso solo dopo parere legale |
| Boost | ordinamento solo nell'app, etichetta "Promosso", tetto di posizioni, disattivabile | sì; N e decadimento da fissare (proposta: max 10 posizioni, dimezza ogni 24 h) |
