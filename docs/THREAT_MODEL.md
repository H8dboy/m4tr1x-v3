# M4TR1X v3 — Threat model

> Chi vogliamo proteggere, da chi, da cosa, e cosa **non** promettiamo.
> Regola: se una protezione non ha un test in `SECURITY_INVARIANTS.md`, qui viene scritta come "intenzione", non come garanzia.

## 1. Per chi esiste M4TR1X

La persona più a rischio è **chi pubblica** (testimone, giornalista, membro della diaspora che documenta) in un Paese dove pubblicare può costare la libertà. Chi guarda è protetto anche lui, ma la priorità, in caso di conflitto (es. velocità contro anonimato), va a chi pubblica.

## 2. Asset

| ID | Asset | Perché conta |
|---|---|---|
| A1 | **Collegamento tra identità M4TR1X e persona reale** (nome, volto, IP, posizione, telefono) | Se si rompe, la persona è esposta. È l'asset numero uno |
| A2 | **Contenuto dei messaggi privati** | Fonti, piani, conversazioni |
| A3 | **Metadati sociali**: chi segue chi, chi scrive a chi, quando | Ricostruiscono reti di persone anche senza leggere niente |
| A4 | **Chiavi dell'utente** (seme, identità, wallet) | Chi le ha *diventa* l'utente |
| A5 | **Saldi e registro H8** | Valore, e anche privacy finanziaria |
| A6 | **Integrità del software** (aggiornamenti, release) | Un aggiornamento falso compromette tutti in una volta |
| A7 | **Chiavi di sistema** (radice emittente, release, keystore, ancore, hub) | Vedi `KEYS.md` |
| A8 | **Disponibilità** dei contenuti e della rete | Censura = togliere la voce |
| A9 | **Identità e posizione degli operatori** (fondatore, chi ospita ancore, hub, edge, nodi) | Pressione, ritorsioni |
| A10 | **Dati sul dispositivo** (cache, bozze, media) | Telefono sequestrato o rubato |

## 3. Attaccanti

| ID | Attaccante | Cosa può fare | Cosa NON può fare (per ipotesi) |
|---|---|---|---|
| T1 | **Sito web malevolo** | Far eseguire JavaScript nel browser dell'utente; DNS rebinding verso 127.0.0.1; richieste cross-origin | Leggere la memoria dell'app; uscire dalla sandbox del browser |
| T2 | **Nodo ostile** (chiunque ne avvii uno) | Mentire, non consegnare, registrare tutto ciò che riceve, mandare dati malformati o enormi, comparire migliaia di volte (Sybil) | Falsificare firme di altri |
| T3 | **Osservatore di rete** (ISP, Wi-Fi pubblico, operatore mobile) | Vedere volumi, tempi, destinazioni IP del traffico | Rompere la cifratura di Tor |
| T4 | **Stato** | T3 su scala nazionale o globale; ordini a operatori nel proprio Paese; sequestro di server e dispositivi; infiltrati; malware mirato | Rompere la crittografia moderna |
| T5 | **Ladro del dispositivo** (o sequestro alla frontiera) | Avere il telefono/PC, acceso o spento | Conoscere passphrase e seme (salvo coercizione: vedi §5) |
| T6 | **Insider** (operatore di hub, ancora, edge o emittente; il fondatore sotto pressione) | Leggere ciò che passa dalla sua macchina, cambiarne il software, consegnare dati | Le chiavi che non tiene (quelle degli utenti, la radice offline) |

## 4. Matrice rischio / mitigazione / residuo

Livelli: **A** alto · **M** medio · **B** basso. "Residuo" = ciò che resta dopo le mitigazioni.

### A1 — Collegamento identità ↔ persona

| Attaccante | Rischio | Mitigazione | Residuo |
|---|---|---|---|
| T1 | M: una pagina fa parlare il browser con il nodo locale per scoprire l'onion o la chiave | Il nodo risponde solo con `Host` locale (I-03), niente CORS `*` (I-04), onion visibile solo al proprietario (I-30) | **B** |
| T2 | A: il nodo registra l'IP di chi si collega | Tutto via Tor (I-01, I-05): il nodo vede un circuito Tor, non un IP | **B** |
| T3 | M: vede che usi Tor | Tor + (facoltativo) bridge obfs4/Snowflake per nascondere l'uso di Tor | **M** (che usi Tor resta visibile senza bridge) |
| T4 | **A**: correlazione temporale con visione globale; ordine all'operatore dell'hub del Paese | Upload a pezzi con pause casuali (riduce, non elimina); hub senza log degli IP (I-18) e senza IP da registrare; metadati tolti sul dispositivo (I-16) | **A contro un avversario globale** (non promesso, §5) · **B** contro un avversario locale |
| T5 | A: il telefono contiene l'app e le chiavi | Chiavi nel portachiavi del sistema; blocco dell'app con PIN o biometria; "modalità discreta" (OPEN_QUESTIONS) | **M**: un telefono sbloccato in mano altrui resta un problema |
| T6 | M: l'operatore dell'hub vede chi carica cosa (chiave pubblica) e quando | Non vede l'IP; metadati già tolti; originali cancellati dopo la conversione | **M**: l'hub sa quale chiave ha caricato quale video (lo sa anche il pubblico) |

### A2 — Contenuto dei DM

| Attaccante | Rischio | Mitigazione | Residuo |
|---|---|---|---|
| T2, T3, T6 | A: leggere i messaggi | NIP-44 end-to-end; nessun kind 4/14 in chiaro (I-19) | **B** |
| T4 | M: conservare oggi, decifrare domani rubando la chiave | Chiave DM separata e ruotabile | **M**: niente forward secrecy (§5) |
| T5 | A: leggere la cronologia sul dispositivo | Database locale cifrato con chiave nel portachiavi | **M** se il telefono è sbloccato |

### A3 — Metadati sociali

| Attaccante | Rischio | Mitigazione | Residuo |
|---|---|---|---|
| T2 | M: il relay vede chi riceve DM e chi segue chi (gli eventi di follow sono pubblici per natura) | Gift wrap: il relay vede il destinatario, non il mittente (I-20); lista di follow privata facoltativa (cifrata per sé) | **M**: il destinatario dei DM e i follow pubblici restano visibili |
| T4 | A: analisi del grafo pubblico | Pseudonimi; nessun H8 nei post (I-15); conti H8 non collegati alla chiave Nostr (I-14) | **M** |

### A4 — Chiavi dell'utente

| Attaccante | Rischio | Mitigazione | Residuo |
|---|---|---|---|
| T1 | M: XSS nell'app che esfiltra chiavi | Chiavi mai nel JavaScript: firma nel lato Rust dell'app; CSP senza `unsafe-inline` (I-31) | **B** |
| T5 | A | Cifratura a riposo (I-06); seme mai sul dispositivo dopo il backup; revoca dal seme (§5 ARCHITECTURE) | **M** |
| T6 | B: nessun server ha le chiavi utente | — | **B** |

### A5 — H8

| Attaccante | Rischio | Mitigazione | Residuo |
|---|---|---|---|
| T2 | M: replica che mente sul registro | Ogni copia si verifica da zero (I-09…I-13); confronto delle teste firmate | **B** |
| T5 | M: spende il saldo | Conferma con PIN/biometria per ogni spesa sopra una soglia; saldi piccoli per natura | **M** (fino al saldo) |
| T6 (emittente) | M: censura una spesa; conia di nascosto | Il conio richiede la radice offline (I-09); conservazione verificata da tutti (I-11); equivocazione scoperta (I-13) | **M**: la censura resta possibile |
| T4 | M: ordine all'emittente di consegnare la mappa chiave Nostr → conto dei creator | La mappa contiene solo i creator che ricevono mance | **M** (dichiarato) |

### A6 — Integrità del software

| Attaccante | Rischio | Mitigazione | Residuo |
|---|---|---|---|
| T2, T3 | A: servire un aggiornamento falso | Manifest firmato con K11 offline (I-21), anti-rollback (I-22) | **B** |
| T6 / T4 | **A**: costringere il fondatore a firmare una release malevola | Build riproducibili dove possibile (I-24): chiunque può ricompilare e confrontare. In futuro: firma 2 su 2 (OPEN_QUESTIONS Q9) | **M** |
| Supply chain (dipendenza npm/cargo compromessa) | A | Lockfile bloccati, audit in CI (I-25), dipendenze minime, elenco di pacchetti vietati | **M** |

### A7–A10 (sintesi)

| Asset | Rischio principale | Mitigazione | Residuo |
|---|---|---|---|
| A7 chiavi di sistema | Furto della radice H8 o della chiave release | Offline, mai in CI, backup diviso (`KEYS.md`) | **M** (persona singola) |
| A8 disponibilità | Sequestro di un hub o di un'ancora | 2 hub con backup reciproco, 3 ancore, cache negli edge e nei nodi | **B–M** |
| A9 operatori | L'IP di un nodo/edge/hub rivelato | Servizi solo onion, bind in locale (I-02), nessun IP nei descrittori (I-29) | **M**: gli operatori degli hub sono persone note per contratto e affitto dei locali |
| A10 dati sul dispositivo | Media e cache leggibili | Database e cache cifrati; cancellazione rapida | **M** |

## 5. Cosa M4TR1X NON promette

Va scritto anche nell'app, in parole semplici.

1. **Non protegge da un avversario che vede tutta la rete** e confronta i tempi (Tor non lo fa, nessuno lo fa a costo ragionevole).
2. **Non protegge un dispositivo infetto.** Se il telefono ha un malware, chi lo controlla vede tutto.
3. **Non protegge un dispositivo sbloccato in mano altrui** né una persona costretta a dare il PIN o il seme.
4. **Non dà forward secrecy ai DM:** se la chiave viene rubata in futuro, i messaggi conservati da qualcuno si possono leggere.
5. **Non nasconde a chi è diretto un DM** al relay che lo consegna.
6. **Non può cancellare un contenuto pubblicato** da tutte le copie: una volta diffuso, altri possono averlo salvato.
7. **Non impedisce all'emittente H8 di rifiutare una spesa** (censura), anche se gli impedisce di rubare o di creare H8 di nascosto.
8. **Non protegge da ciò che l'utente stesso rivela:** volto, voce, luoghi riconoscibili nel video, stile di scrittura.
9. **Non garantisce che un video sia vero.** Il rilevatore AI (se tenuto) etichetta, non certifica.
10. **Non è anonimo verso l'emittente** chi riceve mance come creator: l'emittente sa quale conto corrisponde a quale chiave.
