# M4TR1X v3 — Domande aperte

> Decisioni che spettano al fondatore. Per ognuna: opzioni, raccomandazione e perché.
> Le prime quattro bloccavano parti del piano: **decise il 15/10/2026** (risposte riportate sotto ogni domanda).
> Le domande nate dalle risposte sono Q16 e Q17.

## Bloccanti

### Q1 — Mance ai creator e regola "H8 non scambiabile tra utenti"
Una mancia è un passaggio di H8 da un utente a un altro: le due regole si scontrano.
- **A (raccomandata):** mance permesse solo verso conti "creator" registrati; il creator può spendere gli H8 ricevuti solo nell'ecosistema (shop, distributori, boost), mai convertirli né passarli ad altri utenti.
- B: niente mance; H8 si spende solo verso H8-Group (shop, distributori, boost).
- C: i creator possono incassare in euro → H8 diventa convertibile, con tutte le conseguenze legali (sotto).

**Perché A:** tiene il senso delle mance senza aprire un mercato tra utenti né un'uscita verso l'euro.

> **DECISO (15/10/2026):** mance sì, **solo verso creator e con lo split**; nessun trasferimento libero tra utenti.
> Nel registro esistono quindi solo questi movimenti: conio → utente (acquisto), utente → creator (mancia, divisa nello split), utente/creator → H8-Group (spesa: shop, distributori, boost), rimborso. Un `trasferimento` utente → utente **non esiste come tipo di riga**: le repliche lo rifiutano (nuova invariante I-34).
> Split di oggi nel codice: 50% creator · 30% nodo che ospita · 20% piattaforma. A chi va la quota "nodo" → Q16.

### Q2 — Come si ottengono gli H8 (e il testo sul sito)
- **A (raccomandata):** acquisto in euro **a senso unico** (si compra, non si rivende), solo tramite l'emittente, **dopo** un parere legale (esenzione "rete limitata", PSD2 art. 3 lett. k; esclusioni MiCA). Fino ad allora la rete H8 gira solo in modalità di prova.
- B: H8 solo guadagnati o regalati (nessun incasso), cioè nessun problema legale ma nessun ricavo.
- C: vendita libera subito: **sconsigliata**.

> **DECISO (15/10/2026):** acquisto con **bonifico SEPA manuale a 1,25 € per H8**, **dopo un parere legale (MiCA)**. Fino al parere, H8 gira solo in rete di prova.
> Conseguenze: nessun fornitore di pagamenti (K14 sparisce); l'emittente riconcilia i bonifici a mano e conia a lotti con la radice offline (vedi ARCHITECTURE §6). Il bonifico rivela il nome di chi paga: → Q17.

**Attenzione:** oggi nderja.com dice "1 H8 = 1,25 €, di cui 1,00 di riserva che copre ogni H8 emesso". Parlare di "riserva" fa pensare che gli H8 siano rimborsabili in euro, cioè l'opposto di "non convertibile", e può far ricadere H8 nella moneta elettronica. **Raccomandazione: togliere la parola "riserva" dal sito finché non c'è il parere legale.**

### Q3 — Visibilità del repo nuovo
- **A (raccomandata):** privato fino alla fine della F4 e a una revisione di sicurezza, poi pubblico. L'emittente (`issuer/`) può restare privato in un repo separato.
- B: pubblico dal primo commit.

**Perché A:** quest'anno i fix sono finiti in repo pubblici prima della rotazione delle chiavi; con il privato iniziale non si ripete.

> **DECISO (15/10/2026):** privato ora, pubblico al lancio, **emittente H8 chiuso**.
> Conseguenza: l'emittente va in un **repo privato separato** (`m4tr1x-issuer`), fuori dal monorepo. La **verifica** del registro (`packages/ledger`) resta nel monorepo e sarà pubblica: chiunque può controllare il registro senza vedere il codice dell'emittente. La sicurezza di H8 non deve dipendere dal codice chiuso, solo dalle chiavi e dalle verifiche pubbliche (ADR-0001 e ADR-0007 aggiornati).

### Q4 — Guscio delle app: Tauri 2 + Arti (ADR-0002)
- **A (raccomandata):** Tauri 2 + Arti, con il prototipo F4a come verifica e un piano B già previsto.
- B: tenere Electron (desktop) + Capacitor (mobile), con Tor gestito separatamente.

**Perché A:** un solo guscio e un solo modulo di rete; chiavi fuori dal JavaScript. Il prezzo è imparare un po' di Rust.

> **DECISO (15/10/2026):** sì, **se il prototipo F4a passa**. Criteri di "passa" in MIGRATION_PLAN (F4a). Se non passa: ci si ferma e si sceglie il piano B con un ADR nuovo.

## Da decidere strada facendo

### Q5 — Linguaggio del server
**JavaScript con `@ts-check` (raccomandato, ADR-0012)** / TypeScript completo / Rust.

### Q6 — Diretta video
- **A (raccomandata):** nessuna diretta in v3.0; "quasi-diretta" HLS via hub (20–40 s di ritardo) in una versione successiva.
- B: WebRTC facoltativo **fuori da Tor**, con avviso esplicito: contraddice la promessa, **sconsigliato**.

### Q7 — Ponte verso Mastodon/PeerTube
**Nessuno al lancio (raccomandato, ADR-0010)** / ponte in sola lettura sugli hub più avanti.

### Q8 — Chi gestisce le 3 ancore
Ogni ancora ha bisogno di un responsabile, di un luogo e di un accordo minimo.
- **Raccomandazione:** almeno una ancora gestita da una persona diversa dal fondatore (riduce il "bus factor = 1" e la pressione su una sola persona). Slovenia: va trovato chi. Le ancore conservano solo dati pubblici (descrittori e copia del registro): il rischio legale per chi le ospita è basso.

### Q9 — Custodia delle chiavi più pericolose (K9 radice H8, K11 release, K12 keystore)
- **A (raccomandata per partire):** il fondatore su computer offline; backup di K9 diviso 2-su-3 tra persone di fiducia; K11 in due luoghi fisici; K12 firmata in locale dopo la build (mai nella CI).
- B: firma 2-su-2 per le release (fondatore + una seconda persona). Più sicura contro la coercizione, più lenta.

### Q10 — iOS
- **A (raccomandata):** prima desktop e Android; iOS dopo il prototipo, distribuito come IPA da firmare con il proprio Apple ID (come oggi).
- B: App Store, che richiede revisione Apple, conto sviluppatore, regole su contenuti e pagamenti (gli H8 nell'App Store passerebbero dagli acquisti in-app di Apple).

### Q11 — Recupero dell'identità
- **A (raccomandata):** solo dal seme.
- B: recupero "sociale" (3 contatti fidati su 5 possono aiutarti a recuperare): più umano ma più complesso e attaccabile. Da valutare dopo il lancio.

### Q12 — Originali dei video sugli hub e obblighi legali
- Gli hub di Italia e Albania **ospitano contenuti di terzi**. In Italia (UE) il Digital Services Act chiede a chi ospita contenuti un punto di contatto e una procedura di segnalazione e rimozione ("notice and action"). Per l'Albania va verificato.
- **Raccomandazione:** (1) cancellare gli originali dopo la conversione (meno dati, meno rischi per chi carica); (2) scrivere una politica di segnalazione e rimozione minima prima del lancio; (3) parere legale insieme a Q2.

### Q13 — Rilevatore AI dei video
- **A (raccomandata):** fuori dalla v3.0. Si reintroduce come lavoro dell'hub (non di ogni nodo), con un modello aggiornato tramite release firmate.
- B: tenerlo da subito sugli hub.

### Q14 — Modalità discreta sul telefono
Un'app che non si presenta come "M4TR1X" (icona e nome neutri, apertura con PIN) aiuta in caso di controllo del telefono.
- **Raccomandazione:** sì, in F9, come opzione.

### Q15 — Nome del repo e dominio
- Repo `m4tr1x` (account H8dboy, privato) + `m4tr1x-issuer` (privato per sempre); nderja.com resta la casa e il canale dei download, con copia di `release.json`.

## Nate dalle risposte del 15/10

### Q16 — A chi va la quota "nodo che ospita" (30%) della mancia
Oggi andava a un indirizzo scritto dall'autore nel post (truffabile: corretto il 15/10 dando tutto al nodo locale). In v3 non ci sono indirizzi H8 nei post.
- **A (raccomandata):** la quota va a un **conto "fondo nodi"** di H8-Group; l'emittente la distribuisce ogni mese agli operatori di nodi/edge/hub in base a criteri pubblici (es. uptime verificato dalle ancore). Niente da falsificare al momento della mancia.
- B: al nodo che ha servito il contenuto a chi manda la mancia, con una ricevuta firmata dal nodo. Più "giusta" ma truffabile (un nodo può firmarsi ricevute) e più complessa.
- C: la quota nodo si somma alla piattaforma finché i nodi esterni non esistono.

> **DECISO (15/10/2026):** opzione A, fondo nodi con regole pubbliche.

### Q17 — Privacy di chi compra H8 con bonifico
Con un bonifico l'emittente vede **nome e IBAN** di chi paga. Se poi accredita gli H8 direttamente su un conto, collega persona reale ↔ conto H8 (e, se quel conto manda mance, ↔ comportamento). Per un utente a rischio è un problema serio.
- A: accredito diretto sul conto, rischio dichiarato. Semplice, ma rompe l'anonimato verso l'emittente.
- **B (raccomandata per il lancio):** l'emittente vende **buoni**: il bonifico compra un codice; il codice si riscatta più tardi, da qualsiasi wallet, via Tor. Riduce la correlazione (diverso momento, diverso canale), **ma** l'emittente vede comunque quale codice è stato riscattato su quale conto, quindi il collegamento resta possibile. Va scritto così, senza promettere di più.
- C (obiettivo): **buoni con firma cieca** (RSA Blind Signatures, standard RFC 9474): l'emittente firma il buono senza vederlo, quindi quando viene riscattato **non sa a quale bonifico corrisponde**. È la soluzione vera; aggiunge ~1 fase di lavoro (2–3 giornate) e un'invariante (I-35).
- Raccomandazione: B al lancio in rete di prova, C prima di aprire la vendita reale.

> **DECISO (15/10/2026):** buoni con firma cieca (opzione C) prima di qualsiasi vendita reale (fase F7b).
