# M4TR1X v3 — Invarianti di sicurezza

> Ogni regola è **verificabile** e ha un test che **fallisce se viene violata**.
> I test vivono in `tests/invariants/` (monorepo, vedi `MIGRATION_PLAN.md`) e girano in CI a ogni commit.
> Prima si scrive il test, poi il codice (Fase 2).
>
> Colonna "Fase": la fase del MIGRATION_PLAN in cui il test diventa obbligatorio. Prima di quella fase il test esiste ma è marcato `pending` (e la CI lo mostra).

## Rete

| # | Invariante | Test | Fase |
|---|---|---|---|
| I-01 | **Nessuna richiesta di rete delle app esce fuori da Tor.** | `inv-01-no-clearnet`: (a) statico: nel codice Rust dell'app solo il client Arti può aprire connessioni (`cargo deny`: vietati `reqwest`, `hyper` client, `std::net::TcpStream` fuori dal modulo Tor); nel frontend nessun `fetch`/`WebSocket`/`<img src>` verso host diversi dall'app; (b) dinamico: l'app gira in un namespace di rete con solo la porta SOCKS di prova raggiungibile e si verifica che nessun pacchetto esca altrove | F4 |
| I-02 | **Il nodo è in ascolto solo su loopback** (e sull'onion) se non configurato diversamente. Le funzioni del ruolo hub sono spente sugli altri ruoli. | `inv-02-bind`: avvia il nodo con config vuota, prova a connettersi da ogni interfaccia non-loopback → rifiutato; controlla che `/hub/*` risponda 404 sul ruolo `user` | F2 |
| I-03 | **Il nodo rifiuta richieste con `Host` non ammesso** (DNS rebinding). | `inv-03-host`: `Host: evil.example` → 421 su tutte le rotte; `Host: localhost` → ok | F2 |
| I-04 | **Nessun `Access-Control-Allow-Origin: *`**; origini ammesse solo esatte. | `inv-04-cors`: preflight da origini somiglianti (`localhost.evil.tld`, `x.onion`, LAN) → nessun ACAO; grep statico di `ACAO *` | F2 |
| I-05 | **Il nodo non contatta mai un peer che non sia `.onion`** e solo via SOCKS di Tor. | `inv-05-onion-only`: si inietta un descrittore con URL in chiaro → nessuna connessione aperta (spy sul socket) | F3 |

## Chiavi e firme

| # | Invariante | Test | Fase |
|---|---|---|---|
| I-06 | **Nessuna chiave privata finisce su disco in chiaro.** | `inv-06-keys-at-rest`: dopo creazione identità, post, DM e spesa, si scansiona la cartella dati cercando i byte della chiave (hex, base64, bech32 `nsec`) → zero risultati | F1 |
| I-07 | **Ruoli separati:** una firma fatta con la chiave di un ruolo viene rifiutata dove serve un altro ruolo. | `inv-07-role-separation`: firma identità usata come autorizzazione H8 → rifiutata; chiave wallet usata per un post → rifiutata | F1 |
| I-08 | **Ogni firma ha un'etichetta di dominio e l'identificativo di rete** (`m4tr1x-main` vs `m4tr1x-test`). | `inv-08-domain-sep`: una firma valida in rete di prova non vale in rete principale; una firma "spesa" non vale come "conio" | F1 |
| I-30 | **L'indirizzo onion di un nodo utente è visibile solo al suo proprietario** (i ruoli pubblici lo pubblicano nel descrittore per scelta). | `inv-30-onion-owner`: `GET /node/onion` senza sessione admin → 401 | F2 |

## Registro H8

| # | Invariante | Test | Fase |
|---|---|---|---|
| I-09 | **Un conio senza firma della radice dell'emittente (K9) viene rifiutato** da ogni replica. | `inv-09-mint-root`: riga di conio firmata solo da K10, o da chiave sconosciuta → replica la rifiuta | F7 |
| I-10 | **Una spesa senza l'autorizzazione del wallet del pagante viene rifiutata**, anche se firmata dall'emittente. | `inv-10-spend-auth`: K10 firma una spesa senza `autorizz` o con firma di un altro conto → rifiutata | F7 |
| I-11 | **Conservazione:** somma dei saldi = somma dei coni − rimborsi; nessun saldo negativo, dopo qualsiasi sequenza. | `inv-11-conservation`: test a proprietà con migliaia di sequenze casuali (fast-check) | F7 |
| I-12 | **Doppia spesa impossibile:** due spese con lo stesso `seq_conto` → la seconda è rifiutata. | `inv-12-double-spend`: due richieste concorrenti con stesso seq → una sola riga | F7 |
| I-13 | **Equivocazione scoperta:** se l'emittente firma due righe diverse con lo stesso indice, la replica si ferma e dà l'allarme. | `inv-13-equivocation`: due teste firmate in conflitto → la replica entra in stato `halted` e lo espone | F7 |
| I-14 | **I conti H8 non sono collegabili alle chiavi Nostr** nel registro pubblico. | `inv-14-unlinkable`: nessun campo della riga contiene o deriva dalla pubkey Nostr (test su schema + controllo che `id_conto` sia casuale) | F7 |
| I-15 | **Nessun indirizzo o conto H8 in un evento Nostr pubblicato.** | `inv-15-no-h8-in-events`: il costruttore di eventi rifiuta tag/campi H8; test su tutti i tipi di evento emessi dall'app | F1 |
| I-34 | **Nessun trasferimento tra utenti:** l'unico movimento verso un conto non-creator e non-H8-Group è il conio/riscatto; una mancia verso un conto che non è creator è rifiutata; lo split della mancia è quello in vigore. | `inv-34-no-p2p`: riga utente → utente → rifiutata dalla replica; mancia a conto non creator → rifiutata; mancia con split diverso → rifiutata | F7 |
| I-35 | **Buono cieco:** l'emittente non può collegare un buono riscattato al bonifico che l'ha pagato. | `inv-35-blind-voucher`: dati del momento della firma e del riscatto: nessun valore in comune (test sul protocollo RFC 9474) | F7b |
| I-32 | **Buoni offline entro il tetto:** un edge non accetta più del tetto giornaliero per conto, anche offline. | `inv-32-offline-cap`: buoni oltre il tetto → rifiutati in locale | F10 |

## Contenuti

| # | Invariante | Test | Fase |
|---|---|---|---|
| I-16 | **Nessun metadato identificativo lascia il dispositivo** (GPS, modello, numero di serie, data originale, nome file). | `inv-16-strip`: file di prova JPEG/HEIC/MP4/MOV con GPS e modello → dopo la preparazione all'upload, nessun campo presente (lettura con exiftool in CI) | F6 |
| I-17 | **Un blob si accetta solo se il suo SHA-256 corrisponde.** Vale per app, nodo, edge. | `inv-17-blob-hash`: un nodo che serve byte alterati → l'app rifiuta e passa a un'altra fonte | F2 |
| I-18 | **Hub, edge e nodi non scrivono indirizzi IP nei log.** | `inv-18-no-ip-logs`: si eseguono le operazioni con client da indirizzi di prova; i log non contengono pattern IPv4/IPv6 | F2 |
| I-26 | **Un input malformato non fa cadere il nodo** (frame WS, JSON, eventi, filtri, blob troppo grandi); memoria e risposte hanno un tetto. | `inv-26-fuzz`: fuzzing del relay e delle API per N minuti in CI → processo vivo, memoria sotto soglia | F2 |
| I-27 | **Ogni processo esterno (ffmpeg, exiftool) ha un timeout** e un tetto di memoria. | `inv-27-spawn`: statico (nessuno `spawn` senza opzioni di timeout) + dinamico (video di prova "bomba" → interrotto) | F6 |

## Messaggi privati

| # | Invariante | Test | Fase |
|---|---|---|---|
| I-19 | **I DM escono solo come gift wrap (kind 1059).** Mai kind 4 o 14 pubblicati. | `inv-19-dm-wrap`: dopo l'invio, sul relay di prova ci sono solo kind 1059 (porta il test del 15/10) | F5 |
| I-20 | **La busta è firmata da una chiave usa e getta e ha data sfalsata.** | `inv-20-wrap-ephemeral`: pubkey della busta ≠ mittente, diversa a ogni invio; `created_at` nell'intervallo previsto | F5 |

## Aggiornamenti e release

| # | Invariante | Test | Fase |
|---|---|---|---|
| I-21 | **L'app rifiuta un aggiornamento con firma non valida** o pacchetto con hash diverso. | `inv-21-update-sig`: manifest con firma sbagliata / pacchetto alterato → rifiutato | F8 |
| I-22 | **Niente rollback:** versione più bassa o `seq` del manifest più basso dell'ultimo visto → rifiutato. | `inv-22-rollback` | F8 |
| I-23 | **La CI non pubblica release senza manifest firmato valido** (ML-DSA-65, chiave K11, rete corretta). | `inv-23-release-gate`: il job di pubblicazione fallisce se `release.json.sig` manca, non verifica, è di un'altra rete o di un'altra famiglia | F0 |
| I-24 | **Build riproducibile** (dove possibile): due build dello stesso tag danno gli stessi hash per frontend, binari Rust e contenuto dell'APK. | `inv-24-repro`: job CI che compila due volte in ambienti puliti e confronta | F8 |

## Repo e dipendenze

| # | Invariante | Test | Fase |
|---|---|---|---|
| I-25 | **Nessuna dipendenza con vulnerabilità alte o critiche; lockfile bloccati; pacchetti vietati assenti.** | `inv-25-deps`: `npm audit --audit-level=high`, `cargo deny check`, elenco vietati (es. `request`, `node-fetch` nelle app, `webtorrent`) | F0 |
| I-28 | **Nessun segreto nel repo** (chiavi, `.env`, `.db`, chiavi onion, IP, email personali). | `inv-28-secrets`: gitleaks con regole extra (onion key `hs_ed25519_secret_key`, `nsec1`, IP privati, indirizzi email non `noreply`) su ogni commit e sulla storia intera | F0 |

## Amministrazione e directory

| # | Invariante | Test | Fase |
|---|---|---|---|
| I-29 | **I descrittori della directory contengono solo onion v3** e sono firmati dalla chiave del nodo; scadenza ≤ 7 giorni. | `inv-29-descriptor`: descrittore con IP, firma sbagliata o scadenza lunga → rifiutato da ancora e app | F3 |
| I-36 | **Trasporto unico:** solo il modulo `transport` dell'app apre connessioni di rete, e accetta solo onion v3 validati (ADR-0016). | `inv-36-transport-only`: statico (nessun uso di socket/HTTP fuori da `app/shell/src/transport/`) + la stessa suite di test gira su `ArtiTransport` e `TorProcessTransport` | F4 |
| I-37 | **Il Tor di Windows è solo dell'app:** SOCKS e controllo solo su 127.0.0.1, porte casuali, autenticazione SOCKS e cookie; `tor` termina con l'app; binario con hash fissato. | `inv-37-tor-process`: un client SOCKS senza credenziali viene rifiutato; uccidendo l'app il processo `tor` sparisce; hash del binario confrontato in build | F4 |
| I-38 | **Le schede della directory scadono e si rinnovano:** scadenza obbligatoria ≤ 7 giorni; scheda scaduta rifiutata, non restituita e cancellata dalle ancore; il rinnovo sostituisce la scheda precedente; il nodo rinnova da solo prima della scadenza. | `inv-38-expiry` | F3 |
| I-39 | **Limiti sulle ancore senza porte chiudibili:** intervallo minimo tra aggiornamenti della stessa chiave; **prova di lavoro adattiva** (NIP-13) sulla prima scheda di una chiave nuova, +2 bit a ogni raddoppio delle chiavi nuove nell'ultima ora oltre l'obiettivo, con tetto alla difficoltà; **nessun tetto fisso di chiavi nuove** (chi paga la prova entra sempre); il nodo rilegge la difficoltà richiesta e riprova; tetto di schede attive (spazio); l'ancora accetta solo schede di directory. | `inv-39-anchor-limits` | F3 |
| I-40 | **Due famiglie di firme (ADR-0017):** Schnorr per identità, DM, revoca, nodo; ML-DSA-65 per wallet, emittente, release. Una firma Schnorr non vale mai per un dominio di valore, né una ML-DSA per un dominio social. Ed25519 e minisign assenti dal codice (Ed25519 solo dentro Tor); RSA solo nel modulo buoni. | `inv-40-signature-families` (dinamico + statico) | F1 |
| I-31 | **Nessuna chiave privata utente raggiunge il codice JavaScript dell'app;** CSP senza `unsafe-inline`. | `inv-31-no-keys-in-js`: le API esposte dal lato Rust al frontend non restituiscono mai chiavi (test sui comandi Tauri); CSP verificata | F4 |
| I-33 | **L'admin del nodo è raggiungibile solo da loopback con la chiave admin (K13)**, distinta da ogni altra chiave. | `inv-33-admin`: da onion/porta pubblica → 404; senza K13 → 401 | F2 |

> Numerazione: alcuni numeri (I-30…I-33) sono stati aggiunti dopo aver scritto il threat model. Sono raggruppati per tema, non per numero. **Un numero non si riusa mai**: un'invariante tolta resta nella tabella come "ritirata" con il motivo.
