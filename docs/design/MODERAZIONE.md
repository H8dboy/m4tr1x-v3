# Moderazione automatica: bandierine sui contenuti pubblici

> **Decisione del fondatore, 24/11/2026:** «per i nodi usa un algoritmo come fa TikTok per i contenuti: traccia parole,
> gesti o cose strane e metti la bandierina… se usano linguaggio in codice». Poi: «fai te, non tradire la filosofia e
> non pubblicare niente di pericoloso». Questo documento è la traduzione nella filosofia di M4TR1X; regola **I-65**.

## Le scelte (e perché)

| Domanda | Scelta | Perché |
|---|---|---|
| Che cosa si guarda | **Solo eventi pubblici** di testo: post e discussioni del forum (kind 1), schede di reel, film e musica (kind 1063: descrizione e titolo), commenti/topic (kind 1111), articoli (kind 30023); con il testo anche i **titoli** (`subject`, `title`) | I messaggi privati sono cifrati e devono restarlo: nessuno li può leggere, e nessun codice ci prova |
| Su chi va la bandierina | **Sul contenuto**, mai sulla persona | Un registro di "persone sospette" su un nodo è esattamente ciò che M4TR1X promette di non diventare (stesso motivo di I-43): chi sequestra il nodo se lo porterebbe via. Una persona che scrive in codice si vede dai suoi contenuti con la bandierina, uno per uno |
| Che cosa fa una bandierina | **Niente da sola**: l'app mostra un velo con il motivo («segnalato: possibile truffa») e chi guarda sceglie se aprirlo | Togliere un contenuto è una decisione di una persona, con la procedura DSA (elenco di rimozione firmato, I-51…I-56). Un algoritmo sbaglia, e il DSA chiede comunque motivazione e ricorso |
| Linguaggio in codice | Il testo si **normalizza** prima del confronto: lettere sostituite da numeri e simboli (`s0ld1`), lettere separate (`s.o.l.d.i`), lettere di altri alfabeti uguali a vista (`ѕоldi` cirillico), caratteri invisibili, lettere ripetute. Se una regola si trova solo nella forma normalizzata, la bandierina dice «scritto in codice» e pesa di più | Chi scrive in codice lo fa per sfuggire: il trucco stesso è un segnale |
| "Cose strane" | Segnali che non dipendono da nessun elenco: **caratteri invisibili** dentro le parole, **alfabeti mischiati** nella stessa parola | Non hanno quasi mai un uso innocente nel testo normale |
| L'elenco delle parole | **Non sta nel repo pubblico.** Il codice e il formato sono pubblici; l'elenco vero lo tiene chi modera e arriva al nodo come file di configurazione (poi come elenco firmato, con le chiavi della moderazione) | Un elenco pubblico di parole in codice insegna a chi vuole aggirarlo e diffonde ciò che si vuole fermare. Nei test ci sono solo esempi innocui («soldi facili») |
| Chi "addestra" | **Le segnalazioni degli utenti** (NIP-56, kind 1984) contano per il contenuto segnalato; un moderatore che le conferma trasforma il caso in una regola nuova | È l'"addestramento" di TikTok, ma trasparente: ogni regola ha un id e una categoria, nessun peso nascosto |
| Gesti nei video | **Dopo**, sull'hub (dove ci sono CPU e, a casa, l'AI): solo reel pubblici, stesse bandierine sul contenuto | Il nodo Oracle ha 500 MB di RAM; un modello per i video non ci sta |

## Che cosa NON fa, per costruzione (I-65)

- Non legge buste (kind 1059), sigilli, annunci DM o qualunque kind fuori dall'elenco: esce prima di guardare il testo.
- Non riceve e non restituisce la pubkey dell'autore: l'analizzatore non la nomina nemmeno.
- Non tiene contatori, elenchi o punteggi per persona.
- Non toglie niente: restituisce bandierine (categoria, peso, id della regola, «scritto in codice» sì/no), e basta.

## Pezzi

1. `packages/moderazione`: analizzatore puro (nessuna dipendenza, nessuna rete), normalizzazione, controllo delle regole. **Fatto il 24/11.**
2. Nodo: bandierine calcolate all'arrivo degli eventi pubblici, salvate per id di evento (mai per autore), conteggio delle segnalazioni NIP-56 per evento (persone diverse); il proprietario le vede dal listener locale con K13. **Fatto il 24/11.**
   - Le regole: in `/etc/m4tr1x/node.json` la voce `"regoleModerazioneFile": "/etc/m4tr1x/regole-moderazione.json"`, file con `{ "versione": N, "regole": [{ "id", "categoria", "schema", "peso" }] }`. Senza: solo i segnali senza elenco. Un elenco storto ferma il nodo all'avvio (meglio saperlo subito che moderare a metà).
   - Vederle: `curl -s -H "X-Admin-Key: <chiave>" http://127.0.0.1:<porta>/admin/bandierine` → contenuti (id, kind, data), bandierine e numero di persone che li hanno segnalati; mai chi scrive, chi segnala o il testo.
3. App: velo con il motivo sui contenuti con bandierina. **Fatto il 24/11** (feed). Segnalare dall'app ("Segnala" su post, risposte, discussioni e file, motivo da un elenco chiuso): **fatto il 26/11** (I-67).
   - Il nodo dà alle app `GET /moderazione?ids=…` (al massimo 100 id): per ogni contenuto con bandierine, solo categoria, peso e «scritto in codice». Mai gli id delle regole (direbbero com'è fatto l'elenco), mai chi scrive o chi segnala.
   - L'app controlla la risposta campo per campo (categorie da un elenco chiuso, solo gli id chiesti, niente campi in più); se il nodo non le dà, il feed si vede senza veli e lo dice.
   - Il velo dice il motivo in parole e si apre con «Mostra comunque»; **il possibile sfruttamento di minori non si apre** (resta coperto finché un moderatore non lo valuta): scelta prudente, un falso allarme costa poco, un'apertura sbagliata no.
   - **Forum e reel, 24/11 sera:** velo sulla voce della discussione (titolo, anteprima e apertura), su ogni risposta e sulla tessera di reel/film/musica (titolo e apertura). Il velo dei reel si basa sul TESTO della scheda (titolo e descrizione), non sui fotogrammi: i gesti nei video restano per l'hub.
4. Hub: lo stesso sui reel (fotogrammi) quando c'è l'AI.

## Compromessi dichiarati

- Una normalizzazione aggressiva fa falsi positivi (una regola corta dentro una parola innocente): per questo il confronto "senza spazi" vale solo per regole di almeno 5 lettere, e la bandierina non toglie mai niente.
- Chi scrive su un nodo che non usa le regole non viene segnalato lì: ogni nodo decide le sue, come ogni relay Nostr.
- Le segnalazioni NIP-56 sono firmate da chi segnala (il nodo conta persone diverse, non clic), ma **dal 26/11/2026 restano nel nodo che le riceve** (I-67): nessuna richiesta le restituisce e nessuna sottoscrizione le riceve, quindi chi è segnalato non sa chi l'ha segnalato e gli altri nodi non le copiano. Il proprietario del nodo, che le riceve, può vedere nel database chi ha segnalato: per segnalare senza farsi conoscere nemmeno da lui resta il modulo via Tor della procedura DSA. Riguardano solo un contenuto (niente tag `p`) e scadono da sole in 90 giorni.
