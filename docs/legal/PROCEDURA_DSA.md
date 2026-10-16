# Procedura DSA "notice and action" — M4TR1X v3

> **BOZZA — non è un parere legale; da validare con un avvocato prima di F6.**
> Scritta il 16/10/2026 come precondizione di F6 (decisione del fondatore, OPEN_QUESTIONS Q12 e Q18).
> Dove una norma è citata con il testo, il testo è stato verificato sulla fonte indicata in fondo. Dove c'è scritto **da verificare**, la regola non è stata controllata e non va data per buona.
> Numeri proposti (I-51…I-56, K18) **non** sono ancora in SECURITY_INVARIANTS.md / KEYS.md: entrano lì all'approvazione.

---

## 0. In breve

- Gli **hub** e gli **edge** di H8-Group conservano e diffondono contenuti caricati da altri: per il DSA (Reg. UE 2022/2065) sono un **servizio di hosting**, e quasi certamente una **piattaforma online** (diffondono al pubblico).
- Da piccola impresa, H8-Group ha **gli obblighi di base** (punto di contatto, segnalazione, motivazione, avviso alle autorità per minacce alla vita, termini e condizioni) e **non** quelli aggiuntivi delle piattaforme (sezione 3, artt. 19–28), salvo un punto (art. 24 par. 3).
- La rimozione funziona con un **elenco di rimozione firmato** (chiave nuova K18) che hub ed edge applicano. Le copie su **nodi di terzi** non si possono togliere: va detto apertamente.
- Prima di F6 servono, oltre a questo documento: termini e condizioni scritti (art. 14), punto di contatto pubblicato, comunicazione ad AGCOM, parere legale.

---

## 1. Chi è il "prestatore"

**Cosa dice il DSA (verificato).** Il DSA distingue tre servizi intermediari: semplice trasporto, memorizzazione temporanea (caching), **hosting** (memorizzare informazioni fornite da chi usa il servizio). Una **piattaforma online** è un hosting che, su richiesta di chi usa il servizio, memorizza **e diffonde al pubblico** le informazioni. Gli obblighi si sommano a strati: tutti gli intermediari (artt. 11–15) → hosting (artt. 16–18) → piattaforme (artt. 19–28) → piattaforme molto grandi.

**Chi è chi in M4TR1X.**

| Componente | Chi lo gestisce | Ruolo DSA (proposta) | Note |
|---|---|---|---|
| Hub Italia | H8-Group | hosting + **piattaforma online** | riceve gli originali, converte, diffonde. È il cuore degli obblighi |
| Hub Albania (Albania) | H8-Group (?) | come Italia, **se** gestito dalla stessa entità italiana | **da verificare:** se lo gestisce un soggetto albanese distinto, quel soggetto offre servizi nell'UE senza stabilimento nell'UE → servirebbe un **rappresentante legale nell'UE** (art. 13). Va verificata anche la legge albanese sul commercio elettronico |
| Edge N150 (distributori) | H8-Group | hosting (cache dei segmenti) — fa parte dello stesso servizio | il caching "puro" ha un regime più leggero, ma un edge che conserva e serve contenuti su richiesta è più vicino all'hosting: **da verificare** |
| Ancore | operatori nei 3 Paesi | probabilmente fuori: conservano solo descrittori dei nodi e copia del registro H8, non contenuti degli utenti | **da verificare** per l'ancora gestita da terzi |
| Nodi personali di terzi | chiunque | **non sono H8-Group.** Chi gestisce un nodo che ospita contenuti di altri potrebbe essere a sua volta un hosting | il DSA si applica ai "servizi della società dell'informazione", di norma prestati dietro retribuzione: un privato che fa girare un nodo per sé e per chi segue **probabilmente** non lo è. **Da verificare.** In ogni caso H8-Group non controlla quei nodi e non risponde per loro |
| App | utente | non è un servizio | — |
| Emittente H8 | H8-Group | non ospita contenuti | fuori da questa procedura |

**Piccola impresa (verificato).** L'art. 19 esclude le micro e piccole imprese (Raccomandazione 2003/361/CE) dall'intera sezione 3 (artt. 19–28), **tranne l'art. 24 par. 3**. L'esclusione dura anche 12 mesi dopo aver perso lo status, e non vale per le piattaforme molto grandi. L'art. 15 par. 2 esclude le micro e piccole imprese anche dai rapporti annuali di trasparenza.

**Quindi H8-Group, da piccola impresa, deve:** art. 11 (contatto per le autorità), art. 12 (contatto per gli utenti), art. 14 (termini e condizioni), art. 16 (segnalazioni), art. 17 (motivazione), art. 18 (avviso di reati gravi), art. 24 par. 3. **Non deve** (ma alcune cose conviene farle lo stesso, vedi sotto): reclamo interno (art. 20), segnalatori attendibili, rapporti annuali.

**Problema a monte (da chiarire con l'avvocato).** Oggi H8-Group **non è ancora una società costituita** (risulta chiusa la P.IVA che si pensava di usare). Il prestatore deve essere un soggetto preciso, con nome e indirizzo. Senza, nessuno di questi obblighi ha un titolare: è la prima domanda per il parere.

**Compromesso.** Dichiararsi piattaforma (e non solo hosting) costa poco oggi, grazie all'esenzione per le piccole imprese, ed evita di dover cambiare tutto quando la qualifica verrà contestata o lo status di piccola impresa finirà.

---

## 2. Punto di contatto

**Cosa serve (verificato in parte).**
- **Art. 11** — un punto di contatto per le **autorità** (AGCOM come coordinatore italiano, Commissione, Comitato). AGCOM chiede di comunicarlo a `dsa@agcom.it` o via PEC a `agcom@cert.agcom.it` con oggetto "COMUNICAZIONE PUNTO DI CONTATTO ex art. 11 DSA". Vanno indicate le lingue (proposta: italiano e inglese).
- **Art. 12** — un punto di contatto per gli **utenti**, raggiungibile rapidamente e per via elettronica, con informazioni pubbliche e aggiornate. **Da verificare:** il testo chiede che l'utente possa scegliere un mezzo che non sia **solo** automatico.
- **Art. 13** — rappresentante legale nell'UE **solo** per chi non ha stabilimento nell'UE (riguarda Albania se gestito da un soggetto albanese, vedi §1).

**Come (proposta).**
1. Una **casella email dedicata** (es. `segnalazioni@nderja.com`), letta da una persona, mai inoltrata a un servizio di terzi con analisi automatica.
2. Un **modulo di segnalazione** raggiungibile in due modi:
   - su **nderja.com** (rete normale): chi segnala da lì espone il proprio IP al server del sito, come per ogni sito. Configurare l'hosting del sito perché **non conservi** l'IP nei log del modulo (da verificare cosa permette il fornitore attuale);
   - **dentro l'app e sull'onion dell'hub** (`/segnala`), via Tor: nessun IP.
3. Pagina pubblica "Contatti e segnalazioni" su nderja.com con: nome del prestatore, indirizzo, email, onion, lingue, cosa succede dopo una segnalazione.

**Perché.** Il DSA vuole che segnalare sia facile; la promessa di M4TR1X vuole che non costi l'anonimato. Le due cose stanno insieme se il modulo esiste anche via Tor.

**Compromesso.** L'email non è anonima per chi la usa (il fornitore di posta vede l'IP). Chi vuole restare anonimo usa il modulo via Tor.

---

## 3. Meccanismo di segnalazione (art. 16)

**Cosa chiede la norma (verificato).** Un meccanismo **facile da usare e solo elettronico**, che consenta segnalazioni sufficientemente precise e motivate con questi elementi:
- (a) perché chi segnala ritiene il contenuto illegale;
- (b) dove si trova esattamente (URL o altro identificativo adatto al servizio);
- (c) **nome ed email** di chi segnala, **tranne** per i reati degli artt. 3–7 della direttiva 2011/93/UE (abusi sessuali su minori e materiale pedopornografico);
- (d) una dichiarazione di buona fede sull'esattezza e completezza.

Se c'è un contatto elettronico, si manda **senza ingiustificato ritardo** una **conferma di ricezione** (par. 4) e poi la **decisione**, con le possibilità di ricorso (par. 5). Le decisioni vanno prese in modo tempestivo, diligente, non arbitrario e obiettivo; se si usano mezzi automatici, va detto (par. 6).

**Campi del modulo M4TR1X.**

| Campo | Obbligatorio | Note |
|---|---|---|
| Identificativo del contenuto | sì | uno tra: link `m4tr1x:` dell'app (contiene l'id dell'evento), id dell'evento Nostr (64 hex), hash del blob o del manifest HLS. L'app ha il pulsante "Segnala" che lo compila da solo |
| Categoria | sì | illegale grave (minori, terrorismo, minaccia alla vita) / diritto d'autore / dati personali / diffamazione / altro illegale / contrario ai termini |
| Motivazione | sì | testo libero |
| Nome ed email | sì, **tranne** la categoria "minori" | per il diritto d'autore serve anche chi è il titolare |
| Dichiarazione di buona fede | sì | casella da spuntare con il testo dell'art. 16(2)(d) |
| Risposta via app | facoltativo | invece dell'email, chi segnala può indicare una pubkey Nostr e ricevere conferma e decisione in DM NIP-17 |

**Tempi interni proposti** (non sono nella norma, che dice "senza ingiustificato ritardo"; sono un impegno da scrivere nei termini):

| Caso | Conferma di ricezione | Blocco | Decisione motivata |
|---|---|---|---|
| Minori, terrorismo, minaccia concreta alla vita | automatica, subito | **entro 24 h** dalla ricezione (anche prima della decisione definitiva) | entro 72 h |
| Altro contenuto manifestamente illegale | automatica, subito | entro 72 h | entro 72 h |
| Diritto d'autore | automatica, subito | entro 7 giorni (serve controllare chi è titolare) | entro 7 giorni |
| Contrario ai termini, non illegale | automatica, subito | — | entro 14 giorni |

**Materiale pedopornografico — percorso separato.**
- Non entra nel fascicolo normale: nessuna copia del contenuto viene fatta per "valutarlo"; si blocca l'hash subito.
- **Art. 18 DSA (verificato):** se l'hosting viene a sapere di informazioni che fanno sospettare un reato che minaccia la vita o la sicurezza di persone (avvenuto, in corso o probabile), deve **informare prontamente** le autorità di polizia o giudiziarie degli Stati membri interessati, con tutte le informazioni disponibili.
- **Legge italiana (da verificare il testo esatto):** la legge 38/2006, che ha modificato la legge 269/1998, ha istituito presso la Polizia Postale il **Centro nazionale per il contrasto della pedopornografia online (CNCPO)** e prevede obblighi di segnalazione e di filtro per i fornitori. Il numero dell'articolo (14-bis, 14-ter o 14-quater) e la sua formulazione attuale **vanno controllati su Normattiva**, così come se l'obbligo riguarda anche un hosting oltre ai fornitori di connettività.
- Procedura proposta: blocco dell'hash → segnalazione al CNCPO (o all'autorità che indica l'avvocato) → conservazione dei soli dati che l'autorità chiede di conservare, **per il tempo che indica** (non cancellare prove su cui c'è un'indagine).

**Perché.** I contenuti gravi non possono aspettare la procedura normale; tutto il resto non può essere tolto su semplice richiesta senza valutazione, o il sistema diventa uno strumento di censura per chi segnala in malafede.

**Compromesso.** Chiedere nome ed email (obbligo di legge, tranne per i minori) scoraggia una parte delle segnalazioni anonime. Una segnalazione anonima si accetta comunque e si valuta, ma **da verificare** se basta a far scattare la "conoscenza effettiva" dell'art. 6 (cioè la responsabilità se non si agisce).

---

## 4. Motivazione a chi ha caricato (art. 17)

**Cosa chiede la norma (verificato).** Chi limita un contenuto (rimozione, blocco, retrocessione), sospende pagamenti, sospende il servizio o l'account deve dare a chi l'ha caricato una **motivazione chiara e specifica**. Vale **solo se si conoscono i suoi contatti elettronici** (par. 2). La motivazione contiene (par. 3): cosa è stato fatto, ambito territoriale e durata; i fatti, e se nasce da una segnalazione o da un controllo d'ufficio; se sono stati usati mezzi automatici; la base giuridica (se illegale) o la clausola dei termini (se contrario ai termini); le vie di ricorso disponibili.

**Come in M4TR1X.** Chi carica non ha email: ha una **pubkey**. Una pubkey a cui si può mandare un DM NIP-17 **è** un contatto elettronico. Proposta:
1. Alla rimozione, lo strumento di moderazione genera un messaggio con i campi dell'art. 17(3) e lo manda in **DM NIP-17** alla pubkey dell'autore, firmato con l'identità di moderazione di H8-Group (K18, vedi §6).
2. L'app mostra il messaggio come "Avviso di moderazione" con un pulsante "Contesta".
3. **Nell'avviso non c'è mai** il nome o l'email di chi ha segnalato, salvo quando è strettamente necessario (lo ammette l'art. 17(3)(b): es. un titolare di diritti d'autore che si identifica come tale).

**Reclamo interno.** L'art. 20 (sistema di reclamo interno) sta nella sezione 3, da cui le piccole imprese sono escluse (art. 19). **Proposta comunque:** il pulsante "Contesta" porta a una seconda valutazione da parte di una persona diversa da chi ha deciso (quando esisterà una seconda persona), entro 14 giorni. Costa poco ed è richiesto in ogni caso per il diritto d'autore (§5, da verificare).

**Compromesso.** Il DM fa sapere all'autore che il suo contenuto è stato rimosso **anche** se l'autore non apre più l'app: va bene. Ma l'identità di moderazione diventa una chiave nota a cui tutti possono scrivere: va protetta come un bersaglio di spam (stessi limiti del relay, I-41).

---

## 5. Diritto d'autore della musica (e dei video)

**Cosa dice la direttiva (in parte verificato).** La direttiva UE 2019/790, art. 17, rende responsabili i **prestatori di servizi di condivisione di contenuti online** per le opere caricate dagli utenti, salvo che dimostrino di aver fatto il massimo sforzo per ottenere licenze, per impedire la disponibilità delle opere segnalate dai titolari e di aver rimosso rapidamente su segnalazione (e impedito che ritornino).

**Regime per i servizi nuovi (art. 17 par. 6, verificato su fonti secondarie, da riscontrare sul testo):** per i servizi disponibili al pubblico da **meno di 3 anni** e con fatturato annuo **sotto i 10 milioni di euro**, bastano il **massimo sforzo per ottenere licenze** e la **rimozione rapida su segnalazione motivata**. Se in più superano i **5 milioni di visitatori unici medi al mese** (anno precedente), devono anche impedire che le opere segnalate vengano ricaricate.

**Da verificare:**
- se M4TR1X sia davvero un "prestatore di servizi di condivisione di contenuti online": la definizione (art. 2 n. 6) riguarda chi ospita **una grande quantità** di opere caricate dagli utenti e le organizza e promuove **a scopo di lucro**. All'inizio M4TR1X potrebbe non rientrarci; boost e mance potrebbero essere considerati "scopo di lucro";
- il recepimento italiano (d.lgs. 177/2021, che ha inserito nella legge sul diritto d'autore gli articoli dal 102-sexies in poi) e il ruolo di AGCOM;
- se il meccanismo di reclamo per chi carica (art. 17 par. 9) valga anche per i servizi nuovi.

**Come (proposta, valida in ogni caso).**
1. Al caricamento di un brano o di un video, l'app chiede una **dichiarazione di titolarità**: "Ho i diritti su questo contenuto, oppure è di pubblico dominio / sotto licenza libera (indicare quale)". La dichiarazione va **nell'evento firmato** dall'autore (un tag `license`), così resta legata al contenuto.
2. Segnalazione del titolare = categoria "diritto d'autore" del modulo (§3), con indicazione dell'opera e prova della titolarità.
3. Rimozione rapida (7 giorni) + **blocco del ricaricamento dello stesso file** (stesso hash nell'elenco di rimozione, §6). Il blocco per hash si fa già da subito, perché costa poco, anche se la soglia dei 5 milioni è lontana.
4. Licenze: per partire, niente musica di terzi in catalogo; la ricerca di licenze collettive (SIAE, altri) è una domanda per l'avvocato.

**Compromesso.** Il blocco per hash ferma solo lo **stesso identico file**: un brano ricodificato ha un hash diverso. Un riconoscimento del contenuto (impronta audio) sarebbe più efficace, ma vuol dire analizzare tutto ciò che si carica: per ora **no**, finché la legge non lo impone (soglia dei 5 milioni) e finché non si trova un modo che non tocchi la privacy.

---

## 6. Progetto tecnico della rimozione

### Cosa si può togliere e cosa no

| Dove sta il contenuto | Si può togliere? |
|---|---|
| Hub Italia, hub Albania | **Sì**: blob, segmenti, manifest, evento nel relay dell'hub |
| Edge di H8-Group | **Sì**: applicano l'elenco al prossimo aggiornamento (proposta: ogni 10 minuti) |
| App di chi l'ha già scaricato | No (è sul suo dispositivo). L'app nasconde i contenuti presenti nell'elenco quando lo riceve da un hub, ma non cancella i file dell'utente |
| **Nodi personali di terzi** | **No.** H8-Group non li controlla. Un operatore di nodo può **scegliere** di applicare l'elenco (opzione nel nodo, spenta o accesa a sua scelta) |
| Relay Nostr di terzi fuori da M4TR1X | No |

**Va scritto apertamente** nei termini e nella pagina pubblica: "Rimuoviamo dai nostri server. Non possiamo rimuovere le copie che altri hanno salvato su nodi propri." Promettere di più sarebbe falso.

### Elenco di rimozione

Un documento **in sola aggiunta** (ogni versione contiene tutte le voci precedenti più quelle nuove), firmato, con numero di versione crescente e tag di rete. Ogni voce:

| Campo | Contenuto |
|---|---|
| `type` | `blob` (hash SHA-256 di un pezzo o di un segmento) / `manifest` (hash del manifest HLS: rimuove tutti i suoi segmenti) / `event` (id dell'evento Nostr) |
| `ref` | l'hash o l'id |
| `reason` | **solo un codice** di categoria (es. `csam`, `copyright`, `illegal`, `tos`), mai testo libero |
| `scope` | `all` o un codice di Paese (blocco solo in uno Stato, se una decisione lo richiede) |
| `case` | numero del fascicolo interno (casuale, non dice nulla da solo) |
| `added_at` | data, arrotondata al giorno |

**Mai** nell'elenco: nome, email, IP o pubkey di chi ha segnalato; testo della segnalazione; pubkey dell'autore (si ricava dall'evento, non serve ripeterla).

**Chi lo applica.**
- **Hub:** cancella blob, segmenti e originali; il relay smette di servire l'evento e lo rifiuta se ricaricato; l'upload di un blob con hash in elenco è rifiutato.
- **Edge:** stesso comportamento sulla cache.
- **Nodi di terzi:** solo se l'operatore lo attiva.
- **App:** nasconde i contenuti in elenco nelle viste che passano dagli hub/edge.

**L'elenco non è pubblico.** Un elenco pubblico di hash di contenuti illegali è una mappa per trovarli altrove. Si distribuisce **solo** agli hub e agli edge di H8-Group (e ai nodi che lo chiedono) sul listener autenticato, non su quello pubblico. **Compromesso:** meno trasparenza su cosa viene tolto; la si recupera con i numeri aggregati (§7).

### Quale chiave firma l'elenco: K18 nuova, non la chiave dell'hub (K8)

| | Riusare K8 (hub) | **K18 nuova "moderazione" (raccomandata)** |
|---|---|---|
| Dove vive | sull'hub, macchina sempre accesa che **converte file di sconosciuti con ffmpeg** (la superficie d'attacco più grande del sistema) | sulla macchina di chi decide le segnalazioni, nel vault con passphrase; mai sugli hub |
| Se rubata | l'attaccante firma manifest falsi **e** può far cancellare qualsiasi contenuto da tutti gli hub ed edge | può far nascondere contenuti dall'infrastruttura di H8-Group (censura), non può alterare contenuti né firmare manifest |
| Separazione dei poteri | chi converte decide anche cosa togliere | chi converte e chi modera sono chiavi diverse |

**Proposta K18:**
- Schnorr secp256k1 (famiglia "social" di ADR-0017: è un'identità Nostr che firma l'elenco e manda i DM di §4).
- Chiave pubblica distribuita **dentro le release firmate K11** (configurazione di nodo e app), così un hub non può accettare una K18 falsa.
- Rotazione: nuova chiave in una nuova release; l'elenco viene ri-firmato dalla nuova.
- Se rubata: elenchi falsi che nascondono contenuti da hub ed edge fino alla rotazione. Mitigazione: allarme se l'elenco cresce più di N voci in un'ora; le voci aggiunte sono comunque registrate e reversibili (una voce si toglie con una nuova versione).

**Una voce si può togliere.** Se un reclamo viene accolto, la versione successiva non contiene più la voce, e gli hub possono ripristinare il contenuto dal backup reciproco, se c'è ancora. Per questo l'elenco è "in sola aggiunta di versioni", non "in sola aggiunta di voci".

### Fascicoli e dati di chi segnala

- Il **fascicolo** (segnalazione, decisione, motivazione inviata, eventuale reclamo) sta sulla macchina della moderazione, cifrato a riposo.
- Dati di chi segnala (nome, email): cancellati **6 mesi dopo** la decisione definitiva, salvo procedimenti in corso.
- Fascicolo completo: conservato **12 mesi** (proposta), poi resta solo una riga statistica anonima (categoria, data al mese, esito, tempo di risposta).
- **Da verificare con l'avvocato:** i termini per difendersi da un'azione di responsabilità potrebbero richiedere di conservare più a lungo la prova di "aver agito rapidamente".
- **Nessun IP**: il modulo via Tor non ne ha; quello su nderja.com è configurato per non registrarli (§2).

### Invarianti proposte

| # | Regola | Test che la verificherebbe | Fase |
|---|---|---|---|
| **I-51** | Hub ed edge applicano un elenco di rimozione **solo** se firmato dalla K18 corrente con la rete giusta e con versione maggiore dell'ultima applicata. | `inv-51-takedown-sig`: elenco firmato con K8, con una chiave sconosciuta, con rete sbagliata, con versione vecchia → ignorato e allarme; elenco valido → applicato | F6 |
| **I-52** | Dopo l'applicazione, quanto è in elenco non è più servito da hub ed edge, e un nuovo caricamento dello stesso hash o dello stesso id evento è rifiutato. Un `manifest` in elenco rimuove tutti i suoi segmenti. | `inv-52-takedown-apply`: carica blob, evento e video di prova → elenco → GET 404 su hub ed edge, upload dello stesso hash rifiutato, segmenti del manifest assenti; contenuti **non** in elenco ancora serviti | F6 |
| **I-53** | L'elenco contiene solo i campi previsti (nessun dato di chi segnala, nessun testo libero) e non è mai servito sul listener pubblico. | `inv-53-takedown-private`: validazione dello schema (campo in più → rifiutato) + richiesta dell'elenco dal listener pubblico → 404 | F6 |
| **I-54** | I dati di chi segnala e i fascicoli vengono cancellati alla scadenza. | `inv-54-case-retention`: fascicolo di prova con nome ed email riconoscibili → tempo simulato oltre la scadenza → assenti dal database della moderazione | F6 |
| **I-55** | L'elenco può solo **nascondere o cancellare**: nessuna voce può aggiungere, sostituire o alterare un contenuto. | `inv-55-takedown-only-removes`: voce con `type` non previsto, o con contenuto allegato → elenco rifiutato | F6 |
| **I-56** | Ogni rimozione di un contenuto con autore noto produce un avviso NIP-17 alla pubkey dell'autore con tutti i campi dell'art. 17(3), e senza dati di chi segnala. | `inv-56-statement-of-reasons`: rimozione di prova → sul relay compare un kind 1059 per l'autore; aperto con la chiave di prova contiene i campi richiesti e non contiene nome/email del segnalante | F6 |

---

## 7. Trasparenza

**Cosa è obbligatorio (verificato in parte).**
- I **rapporti annuali** di moderazione (art. 15 par. 1) **non** si applicano alle micro e piccole imprese (art. 15 par. 2).
- Gli obblighi di trasparenza delle piattaforme (art. 24) stanno nella sezione 3, esclusa per le piccole imprese, **tranne il par. 3** (art. 19). **Da verificare** che il par. 3 sia l'obbligo di comunicare al coordinatore (AGCOM) o alla Commissione, se lo chiedono, le informazioni sul numero di utenti attivi.
- **Art. 14 (vale per tutti):** i termini e condizioni devono dire quali restrizioni si applicano ai contenuti, come si modera (anche con mezzi automatici) e come funziona il reclamo, in linguaggio chiaro. **Va scritto prima di F6.**

**Cosa pubblicare su nderja.com (proposta, anche se non obbligatorio).**
1. **Termini e condizioni** (art. 14), in italiano e inglese, con la frase onesta sui nodi di terzi (§6).
2. **Contatti e segnalazioni** (artt. 11–12): prestatore, indirizzo, email, onion, lingue, tempi interni (§3).
3. **Numeri di moderazione**, ogni 6 mesi: segnalazioni ricevute per categoria, rimozioni, tempo medio di risposta, reclami accolti, segnalazioni alle autorità. Solo numeri aggregati, nessun hash, nessun contenuto.
4. **Utenti attivi medi al mese**, calcolati senza tracciare nessuno (es. numero di pubkey distinte che hanno pubblicato almeno un evento sugli hub nel mese, arrotondato): serve se AGCOM lo chiede (art. 24 par. 3) e per sapere quando ci si avvicina alle soglie.

**Perché.** Un sistema che chiede fiducia sulla privacy deve mostrare **quanto** rimuove, senza mostrare **cosa**.

---

## 8. Domande per il parere legale

Da unire al parere già previsto per Q2 (vendita H8, MiCA) e Q12 (hosting).

- **Soggetto:** quale soggetto giuridico fa da "prestatore" finché H8-Group non è costituita? È possibile partire con F6 in rete di prova senza un prestatore definito?
- **Qualifica:** hub ed edge sono hosting + piattaforma online? Un edge che fa cache rientra nell'hosting o nel caching (art. 5)?
- **Albania:** se l'hub albanese è gestito dalla stessa società italiana, basta lo stabilimento in Italia? Se è un soggetto albanese, serve un rappresentante legale nell'UE (art. 13)? Quali obblighi impone la legge albanese?
- **Nodi di terzi:** un privato che gestisce un nodo è un prestatore di servizi della società dell'informazione? H8-Group ha qualche responsabilità per i nodi che compaiono nella directory delle ancore?
- **Ancore:** l'ancora gestita da un terzo, che conserva solo descrittori firmati e copia del registro H8, ha obblighi DSA?
- **Piccola impresa:** conferma dell'esenzione dalla sezione 3 e contenuto preciso dell'art. 24 par. 3.
- **Segnalazioni anonime:** una segnalazione senza nome ed email fa scattare la "conoscenza effettiva" (art. 6)?
- **Pedopornografia:** articolo esatto della legge 269/1998 (come modificata dalla 38/2006) sugli obblighi verso il CNCPO, se vale per un hosting, e cosa va conservato in caso di segnalazione.
- **Art. 18:** a quale autorità si segnala in pratica una minaccia alla vita scoperta su un hub italiano (Polizia Postale?), e per un hub albanese.
- **Diritto d'autore:** M4TR1X è un "prestatore di servizi di condivisione di contenuti online" (direttiva 2019/790, art. 2 n. 6) all'inizio? Boost e mance lo rendono "a scopo di lucro"? Come si applica il d.lgs. 177/2021 e il regime per i servizi nuovi (art. 17 par. 6)? Serve un reclamo per chi carica (art. 17 par. 9) anche per i servizi nuovi? Licenze collettive (SIAE) necessarie per la musica?
- **Conservazione:** per quanto tempo conservare fascicoli e dati di chi segnala per potersi difendere, senza tenere troppi dati personali (GDPR)?
- **Elenco non pubblico:** tenere riservato l'elenco degli hash rimossi è compatibile con gli obblighi di trasparenza?
- **Termini e condizioni:** revisione del testo prima della pubblicazione.

---

## Prima di iniziare F6 (checklist)

- [ ] Parere legale ricevuto sulle domande del §8
- [ ] Soggetto "prestatore" definito
- [ ] Termini e condizioni scritti e pubblicati (art. 14)
- [ ] Pagina "Contatti e segnalazioni" pubblicata (art. 12) e punto di contatto comunicato ad AGCOM (art. 11)
- [ ] Casella email e modulo via Tor pronti
- [ ] K18 generata (cerimonia come le altre chiavi, KEYS.md) e chiave pubblica nella configurazione firmata
- [ ] Invarianti I-51…I-56 scritte come test (in attesa) **prima** del codice di F6
- [ ] Percorso CSAM verificato con l'avvocato (autorità, cosa conservare)

---

## Fonti consultate

- Testo del Regolamento (UE) 2022/2065, articoli 15, 16, 17, 18, 19 (versione inglese consultata il 16/10/2026): [art. 15](https://www.eu-digital-services-act.com/Digital_Services_Act_Article_15.html), [art. 16](https://www.eu-digital-services-act.com/Digital_Services_Act_Article_16.html), [art. 17](https://www.eu-digital-services-act.com/Digital_Services_Act_Article_17.html), [art. 18](https://www.eu-digital-services-act.com/Digital_Services_Act_Article_18.html), [art. 19](https://www.eu-digital-services-act.com/Digital_Services_Act_Article_19.html). **Da riscontrare** sul testo ufficiale italiano su EUR-Lex (CELEX 32022R2065): la pagina non si è caricata durante la stesura.
- AGCOM come coordinatore dei servizi digitali e comunicazione del punto di contatto ex art. 11: [comunicazione AGCOM 08/03/2024](https://www.agcom.it/sites/default/files/migration/comunicazione/Comunicazione%2008-03-2024.pdf), [disposizioni del DSA rilevanti (AGCOM)](https://www.agcom.it/sites/default/files/media/allegato/2024/Disposizioni%20del%20DSA%20rilevanti%20per%20i%20soggetti%20P2B.pdf).
- Direttiva (UE) 2019/790, art. 17 par. 6 (soglie dei servizi nuovi), solo da fonti secondarie: [McDermott Will & Emery](https://www.mwe.com/?p=84391), [Stanford WILMap](https://wilmap.law.stanford.edu/node/31133), [Consiglio UE ST-15441-2024](https://data.consilium.europa.eu/doc/document/ST-15441-2024-INIT/en/pdf). **Da riscontrare** su EUR-Lex.
- Legge 38/2006 e CNCPO, solo da fonti secondarie: [Polizia di Stato](https://www.poliziadistato.it/articolo/pdf/455), [Altalex](https://www.altalex.com/documents/news/2007/03/29/pedopornografia-su-internet-obbligo-del-provider-di-oscurare-i-siti-segnalati). **Da riscontrare** su Normattiva.
