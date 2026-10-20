# Nota di ripresa — 20/10/2026

> Fonte di verità: codice e test.

## Stato
- Repo privato `H8dboy/m4tr1x-v3`. Test locali (Node 22.23.3, 20/10, con tor vero, runner nuovo che conta come fallito un file che non risponde): **174 verdi, 0 falliti, 0 saltati**, 26 regole attive, 0 divergenze doc↔codice (`npm run report`).
- Sabotaggio completo: **64/66 prese** (16/10, con tor vero; 2 non applicabili poi corrette) (`docs/generated/sabotage.json`). Corretta una prova invecchiata (I-38 purge: il testo cercato era cambiato con I-41).
- Fasi chiuse: **F0, F1, F2, F3**. F4a aperta (serve telefono Android; Arti non regge su Windows → ADR-0016).

## Fatto il 16/10
1. KEYS.md e report: K5/K13 nel vault, passphrase obbligatoria, compromesso scritto; debolezze 3–5 e 13 chiuse nel report.
2. **Musica e topic dentro v3.0**: musica = Blossom + HLS audio in F6 (I-16 estesa all'audio); topic = NIP-72 in nuova fase **F6b** (proposta I-42). Stima totale ~45–59 giornate.
3. **Q18 deciso**: procedura DSA notice and action scritta e approvata **prima di iniziare F6** (precondizione in MIGRATION_PLAN). Q19 (moderazione topic) aperta.
4. **Bozza** `docs/design/PEZZI_MANCANTI.md`: monitoraggio, registrazione creator (mappa minima e cifrata con K17, nessuno storico, proposta I-50), shop fisico (K16, indirizzi cancellati dopo 30 gg), boost (ordinamento nell'app, "Promosso", tetto posizioni). **Da approvare.**

## Fatto il 16/10, seconda parte (F4 iniziata)
5. GitHub Actions fissate per hash (checkout v7.0.1, setup-node v7.1.0), `persist-credentials: false`, cache spenta; I-25 lo controlla.
6. Bozza `docs/legal/PROCEDURA_DSA.md` (proposte I-51…I-56, K18). Punto chiave del fondatore: **Q20 SRLS** prima di hub reali, vendita H8, shop.
7. **I-37 riscritta (opzione A):** tor C non può rendere obbligatoria l'autenticazione SOCKS (provato); garanzie reali = `OnionTrafficOnly` (rifiuto con codice 2), circuiti isolati, solo 127.0.0.1, cookie, tor muore con l'app, hash fissato. ADR-0016 corretta.
8. **Crate `app/transport`** (I-36: percorso aggiornato): `OnionV3` validato (vettore indipendente Python), trait `Transport`, `TorProcessTransport`. Con tor vero: onion di prova raggiunto in ~37 s. `app/tor-windows.lock.json` con gli hash del Tor Expert Bundle 15.0.21 (firma GPG delle somme ancora da verificare, F8).
9. **Sabotaggio irrobustito:** timeout = "interrotta" (non più "presa"), `timeoutMs` per prova; test I-37 "solo onion" ora pretende il codice 2; prova "muore con l'app" senza esempio di cargo (non veniva ricompilato); test I-39 riprova irrobustito (16 bit + log). **48/48 prese** da campagna vera con tor.
10. Prove locali con tor: `M4TR1X_TOR_BIN=<tor.exe verificato>`; il tor di F4a è nello scratchpad della sessione del 15/10 (non nel repo).

## Fatto il 16/10, terza parte
11. **Pezzi mancanti APPROVATI** (monitoraggio 6 prove/giorno, 2 ancore su 3; creator; shop 30 gg + **ritiro al distributore Elea senza indirizzo**, I-57; boost 10 posizioni, dimezza in 24 h, solo "scopri"). I-42…I-50, I-57, K16, K17 ufficiali (pending); fasi nuove F2b, F3b, F7c.
12. CI rossa su e3974f1 per email nella bozza DSA (scanner lanciato PRIMA di git add: errore mio) → corretta in 7adcbf0, CI verde; I-37 verde anche su Linux con tor vero. **Regola: scanner sempre DOPO git add.**
13. **Guscio Tauri `app/shell` + `app/ui`:** CSP stretta (oggetto, niente unsafe-*), freezePrototype, devtools spenti, app manifest con elenco comandi (solo `stato`), capability senza core/plugin; si avvia e mostra lo stato via invoke. **I-31 e I-36 attive (parte statica)**, 9 sabotaggi nuovi. Report: 147 verdi, 24 regole attive; sabotaggio **57/57**.

## Fatto il 16/10, quarta parte (identità)
14. **`app/identity`** (Rust): BIP-39 + HKDF, stesse chiavi di packages/keys su tutti i vettori congelati (wallet ML-DSA-65 compreso); portachiavi di Windows solo identity/dm/wallet (mai seme, parole, revoca); creazione con verifica di **3 parole a caso**; recupero.
15. **Finestra del seme (I-58, opzione A del fondatore):** capability e pagina proprie, controllo dell'etichetta nel lato Rust, copia/incolla/menu bloccati, **screenshot bloccati** (affinità 0x11 letta da Windows, finestra assente dalla cattura). Provata a mano dal fondatore: identità creata, poi cancellata dal portachiavi. Rischio residuo scritto: l'accessibilità (e un malware dello stesso utente) legge le parole. I-31 con l'eccezione unica.
16. Bug trovato provando l'app: su Windows il comando che apre una finestra deve essere `async`.
17. Sabotaggio: campagna salvata **64/66** (2 prove di I-31 non più applicabili dopo il cambio dei permessi: aggiornate e prese da sole, entrano nella prossima campagna).
18. **Debito:** `tests/integration/two-nodes.test.js` via Tor è instabile nei giorni di rete lenta (2 fallimenti il 16/10, poi 2/2 verde): allungare la finestra o misurare i tempi.

## Fatto il 16/10, quinta parte (prova dei due nodi e tor del nodo affidabili)
19. **Prova dei due nodi via Tor a fasi misurate** (tor-a, pubblicazione, tor-b, rete, evento, file) con limiti scelti sui tempi misurati e log della fase che si blocca; eventi HS_DESC riassunti a ogni giro.
20. **Cache del consenso di tor tra i test** (idea del fondatore): solo `cached-*`, mai chiavi né `state`; M4TR1X_TOR_CACHE=0 la spegne.
21. **Nodo: riscaldamento in produzione** (decisione del fondatore): tor senza onion → bootstrap 100% → ADD_ONION con **K6 nel vault** (prima in chiaro in `hs/`). Pubblicazione = almeno 6 UPLOADED del periodo **attuale** (l'attuale lo dice tor via GETINFO: in 1 prova su 3 tor carica prima il prossimo, l'assunzione sull'ordine era FALSA); UPLOAD appesi non bloccano; annuncio di riserva dopo 300 s; riconnessione automatica della porta di controllo; tor morto o bootstrap non completato → il nodo si chiude con errore (`onFatal`), lo riavvia il sistema.
22. **I-59** nuova (porta di controllo: 127.0.0.1, cookie solo dell'utente, onion mai nei log, + tutto il punto 21): 10 prove con tor vero; I-40 con eccezione stretta per tor.js (etichette PEM e `ED25519-V3`, nessuna libreria Ed25519).
23. **Misure:** riserva dei 5 minuti 2/12 sia prima sia dopo il riscaldamento, ma a freddo la pubblicazione passa da minuti a secondi. Il periodo attuale a volte resta sotto 6 conferme per minuti: comportamento della rete Tor, coperto dalla riserva (il fondatore: non inseguire oltre).
24. **CI:** passo di riscaldamento, I-59 con tor vero, misura a freddo NON bloccante nel riepilogo del job; `npm run misura:tor` aggiunge allo storico `docs/generated/tor-tempi.json`, mostrato nel report (sez. 11.2b).
25. **Da fare con l'installer del nodo:** programma da riga di comando con `onFatal` → uscita con codice 1, unità systemd con `Restart=on-failure`, servizio Windows con ripristino.

## Fatto il 20/10 (lo strumento che dice "verde")
26. Correzione alla nota del 16/10: GitHub era a `df7316f` (non `f50fc96`) e la sua CI era **rossa** (due nodi via Tor, "B non ha ricevuto l'evento da A").
27. **Il report diceva "160 verdi, 0 falliti" con un file che non aveva girato:** I-37 si è fermato a 120 s senza stampare niente e veniva contato 0/0. Provato con Node 22.23.3: `node --test` dà VERDE anche a un file senza test e a un file che esce con `process.exit(0)` a metà (i test non eseguiti spariscono). **Regola del fondatore: un test che non risponde conta come fallito, sempre.**
28. **Runner unico `tools/report/esegui.js` + `esito.js`**, usato da report, `npm test` e CI (job test e tor): file fermato, codice di uscita ≠ 0, senza riepilogo, 0 test, annullati, nessun test registrato → FALLITO con le ultime righe dell'output; ogni file gira senza isolamento (`--experimental-test-isolation=none`) così l'uscita anticipata porta via il riepilogo. Il report esce con 1 se c'è un fallito. Job tor con `--senza-saltati` (lì una prova saltata = non ha girato). **Tempi massimi da una sola fonte** (`TEMPI`: 120 s per test senza timeout proprio, 1 h per file), tolti i valori a mano dalla CI. Prove: `tests/unit/report-esito.test.js` con processi veri (`tests/fixtures/report/`), sabotaggi `report-*` **6/6 presi**.
29. Il runner nuovo ha subito preso un rosso vero: due nodi via Tor fermo nella **fase "file"** a 60 s (rete già su, evento in 7 s). Il fondatore: **non liquidarlo come rete lenta**, indagare con i log del trasferimento. Quei log non esistevano: ora `fetchVerified` segnala le fasi (connessione pronta, risposta HTTP, byte ricevuti, con i ms; mai indirizzi né hash) e il test le stampa tutte, anche da verde. Giro di prova: file in 0,8 s (connessione 307 ms, risposta 704 ms).
30. Stesso giro: fase **rete a 299,8 s su 300** (B: REQUESTED×31, FAILED NOT_FOUND×30; A annunciato con la riserva dei 5 min). Al limite: da guardare se ricapita.

31. CI di `f39ee26` (primo giro col runner nuovo) **verde**: job tor 0 saltati (due nodi 2/2, I-59 10/10, I-37 2/2); job test 163 verdi, 7 saltati (tor/cargo assenti lì).
32. **Blob, decisioni del fondatore applicate:** (1) tempo massimo TOTALE **30 s per tentativo** (`TEMPO_MASSIMO_BLOB_MS`, chiude anche una connessione SOCKS in corso); (2) **3 tentativi**, attese 2 s e 4 s (`TENTATIVI_BLOB`, `ATTESA_BLOB_MS` in peers.js), niente tentativi paralleli dello stesso file; un file arrivato SBAGLIATO (hash diverso, troppo grande) non si ritenta; (3) **"file servito" solo nei test**: è la funzione `onBlobServitoForTests`, che un file di configurazione JSON non può contenere (`true`/stringhe si scartano) e non passa dal log del nodo. Prove `tests/integration/blob-replica.test.js` (peer che manda un byte alla volta, file che arriva tardi, stop dopo 3, JSON) + sabotaggi `blob-*` **4/4 presi**. La prova dei due nodi ora dice anche se la richiesta è arrivata ad A.

## Prossimo

- **Per l'installer del nodo (punto 25):** la configurazione di produzione si legge SOLO da file (JSON), così nessuna opzione `…ForTests` che sia una funzione può accendersi.

1. Campagna completa di sabotaggio (lunga: le prove I-59 con tor durano ~10 min ciascuna).
2. Se ricapita il rosso nella fase "file": leggere le righe "trasferimento del file su B" e "su A" dell'errore.
3. Poi F4: feed.

- OK del fondatore sulla bozza PEZZI_MANCANTI (tabella in fondo) → numeri I-42…I-50, K16, K17 entrano in SECURITY_INVARIANTS/KEYS.
- Scrivere la procedura DSA (precondizione di F6).
- **F4 continua:** feed/post/profilo verso un nodo di prova (firma nel lato Rust con l'identità salvata), collegare `TorProcessTransport` al guscio (stato reale), prova dinamica di I-31 con seme di prova, I-01 dinamica (namespace su Linux), `ArtiTransport` per Linux, build del guscio e `cargo deny` in CI.

## Fuori dal repo, ancora aperto
- Sito nderja.com: correggere "identità ML-DSA", "codice pubblico", download Kalendae rotto.
- Telefono Android per F4a.
- Repo v2: rotazione di `h8identity.enc` mai fatta.
