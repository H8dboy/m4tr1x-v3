# M4TR1X v3 — Piano di migrazione

> Dal codice attuale (4 repo, ~60% secondo la stima di luglio, irrobustito il 15/10/2026) a M4TR1X v3.
> **Zero utenti: si resetta tutto.** Repo nuovo con storia pulita; chiavi, onion e keystore nuovi.
> I repo vecchi restano in sola lettura come archivio (privati), da cui si copia codice a mano, file per file, rileggendolo.

## 1. Struttura del repo nuovo (monorepo, ADR-0001)

```
m4tr1x/
├─ docs/                    questo materiale (ARCHITECTURE, THREAT_MODEL, ADR…)
├─ packages/
│  ├─ protocol/             tipi di evento, etichette di dominio, firma/verifica, costruttore di eventi
│  ├─ keys/                 seme BIP-39, derivazione HKDF per ruolo (usato da app e test)
│  └─ ledger/               formato delle righe H8, verifica completa del registro (usato da nodo, app, emittente)
├─ node/                    programma server unico: relay, Blossom, Tor, directory, ruoli user/edge/hub/anchor
├─ app/
│  ├─ ui/                   interfaccia web (HTML/CSS/JS, niente framework pesanti)
│  └─ shell/                Tauri 2 (Rust): rete solo via Arti, chiavi, aggiornamenti
├─ tools/
│  └─ release/              firma offline del release.json, verifica, cerimonia chiavi
├─ tests/
│  └─ invariants/           un file per ogni invariante I-xx
└─ .github/workflows/       CI: test, invarianti, audit dipendenze, gitleaks, blocco release non firmate

m4tr1x-issuer/              repo PRIVATO separato (decisione Q3): emittente H8, buoni, riconciliazione SEPA
```

**Perché monorepo:** vedi ADR-0001. In breve: oggi la copia del server dentro electron era rimasta indietro e aveva buchi già chiusi altrove. Con un solo repo non può succedere.

## 2. Cosa si porta, cosa si riscrive, cosa si butta

Stime in **giornate di lavoro** (una giornata = una sessione lunga con Claude, circa 4–6 ore). Sono stime, non promesse: le fasi con codice nuovo (Rust, emittente) hanno l'incertezza più alta.

### Si porta (copiato e riletto, con i suoi test)

| Da | Cosa | Dove va | Stima |
|---|---|---|---|
| node `server/relay.js` | Relay Nostr irrobustito (limiti, validazione, effimeri, bind locale) | `node/relay/` | 1 |
| node `server/nostr-verify.js` | Verifica Schnorr compatibile @noble 2.x | `packages/protocol/` | 0,5 |
| node `server/nostr.js` (parte DM) | NIP-17 con nip59 + `poolSubscribe` | `packages/protocol/` | 1 |
| node `server/tor_node.js`, `tor_socks.js` | Gestione di `tor` sul server, SOCKS | `node/tor/` | 1 |
| node Blossom (upload/serve blob) | Archivio per hash | `node/blobs/` | 1 |
| node `server/video_host.js` | Pipeline ffmpeg → HLS (con timeout) | `node/roles/hub/` | 1,5 |
| node middleware di sicurezza | Guardia `Host`, CORS esatto, sessioni, rate limit | `node/http/` | 1 |
| node frontend: dispatcher senza handler inline, CSP | Base per l'interfaccia | `app/ui/` | 1 |
| Redesign v3 (CSS, icone) | Aspetto grafico | `app/ui/` | 1 |
| Test di sicurezza attuali (XSS, CSP, CORS, firma, DM) | Diventano test delle invarianti | `tests/invariants/` | 1,5 |
| **Totale porto** | | | **~10,5** |

### Si riscrive

| Cosa | Perché | Stima |
|---|---|---|
| Registro H8 (`h8token.js`, `ledger_sync.js`) | Il modello cambia del tutto: da "registro per nodo" a "registro dell'emittente replicato" | 5–8 |
| Identità e chiavi | Seme + derivazione per ruolo + portachiavi del sistema; niente chiave Nostr derivata dal wallet | 3–4 |
| Directory | Descrittori firmati + replica tra ancore al posto dell'head | 3–4 |
| Guscio app (Tauri 2 + Arti) | Al posto di Electron e Capacitor | 8–12 |
| Aggiornamenti firmati | Non esistono oggi | 3–4 |
| Upload a pezzi + pulizia metadati sul dispositivo | Oggi l'upload è un blocco unico; la pulizia è solo sul server | 3–4 |
| Buoni offline per i distributori | Non esistono | 2–3 |
| CI completa | Oggi la CI non esegue test (build.yml con 0 esecuzioni sul nodo) | 2 |
| **Totale riscrittura** | | **~29–41** |

### Si butta

| Cosa | Motivo |
|---|---|
| Repo `m4tr1x-head` (Postgres) e `head-server` incorporato | Niente head centrale (ADR-0003, ADR-0011) |
| Electron e Capacitor | Un solo guscio (ADR-0002) |
| Adattatori Mastodon / PeerTube / Funkwhale, `universal_post.js` | Un protocollo per funzione (ADR-0010) |
| WebTorrent / `p2p.js` / tracker | Rivela gli IP (ADR-0010) |
| Diretta WebRTC (`livestream.js`) | Non passa da Tor (ADR-0006) |
| `h8shop.js` (vendita fiat dentro i nodi) | La vendita, se ci sarà, è solo dell'emittente (OPEN_QUESTIONS Q2) |
| `session_guard.js` | Sostituito dai numeri di sequenza dell'emittente |
| `crowdtrain` / addestramento distribuito del modello AI | Fuori dal nucleo; da rivalutare (OPEN_QUESTIONS Q13) |
| `model_updater.js` | Sostituito dal sistema di aggiornamenti firmati |
| NIP-04 | Si riparte da zero, nessun messaggio vecchio da leggere |
| Tag `h8` sui post, `deriveSigningKey` | Identità e denaro separati (ADR-0008) |

**Totale stimato: ~40–52 giornate di lavoro**, cioè, con il lavoro in officina, indicativamente **4–7 mesi** a 2–3 sessioni a settimana. L'incertezza maggiore sta nel guscio Tauri + Arti su mobile (F4a/F9).

## 3. Ordine di costruzione

Regole per ogni fase:
- **Prima** si scrivono i test delle invarianti della fase (devono fallire), **poi** il codice.
- Una fase è "fatta" solo se: test di accettazione verdi in CI, invarianti della fase attive (non più `pending`), nessun segreto nei commit, rapporto di fine fase scritto (cosa è fatto, cosa **non** è verificato, cosa resta).
- Nessuna fase si chiude con "dovrebbe funzionare".

| Fase | Obiettivo | Test di accettazione | "Fatto" quando |
|---|---|---|---|
| **F0 — Fondamenta** | Repo nuovo, struttura, CI con: test, `npm audit`/`cargo deny`, gitleaks, blocco release non firmate; tutte le invarianti create come `pending` | Un commit con un finto segreto viene bloccato; un finto rilascio senza firma viene bloccato | I-23, I-25, I-28 attive e verdi; CI rossa se si viola |
| **F1 — Protocollo e chiavi** | `packages/keys` (seme, derivazione per ruolo), `packages/protocol` (eventi, etichette di dominio, verifica, costruttore che rifiuta H8 nei post) | Vettori di prova: stesso seme → stesse chiavi; firma di un ruolo rifiutata in un altro | I-06, I-07, I-08, I-15 verdi |
| **F2 — Nodo base** | Programma `node` con relay + Blossom + guardie HTTP + Tor (ruolo `user`) | Due nodi di prova si scambiano eventi e blob via Tor (in CI: Tor reale o in rete di prova `chutney`) | I-02, I-03, I-04, I-17, I-18, I-26, I-30, I-33 verdi |
| **F3 — Directory** | Descrittori firmati, ruolo `anchor`, replica tra 3 ancore di prova, lettura e unione lato client | Un'ancora spenta: la directory resta completa; un'ancora che mente (nasconde un nodo) viene compensata dalle altre | I-05, I-29 verdi |
| **F4a — Prototipo Tauri + Arti** (rischio) | Prova minima: app desktop + Android che apre un servizio onion via Arti e scarica un blob | **Passa se, su Windows e sul tuo telefono Android reale:** (1) primo avvio a freddo: connessione a un onion di prova entro 60 s nel 90% di 10 tentativi; (2) download di un blob da 10 MB via onion a ≥ 0,5 MB/s medi; (3) 30 minuti di uso senza crash né perdita di circuito non recuperata; (4) APK con Arti incluso ≤ 40 MB; (5) un'analisi del traffico (PCAPdroid su Android, Wireshark su PC) non mostra connessioni fuori da Tor | Decisione documentata in un ADR: Arti va, oppure piano B (tor nativo) |
| **F4 — App base** | Guscio Tauri desktop con modulo `transport` unico (ADR-0016: Arti su Linux/macOS, `tor` classico su Windows): identità (crea/recupera dal seme), feed, post, profilo, firma nel lato Rust | Flusso completo su desktop con nodo di prova; nessun pacchetto esce fuori da Tor | I-01, I-31, I-36, I-37 verdi |
| **F5 — DM** | NIP-17 nell'app, chiave DM separata e ruotabile | Due utenti di prova si scrivono; sul relay solo kind 1059 | I-19, I-20 verdi |
| **F6 — Video** | Pulizia metadati sul dispositivo, upload a pezzi, ruolo `hub` (HLS, manifest firmato, backup tra 2 hub), ruolo `edge` (cache) | Video di prova con GPS: caricato, convertito, visto da un'altra app tramite edge; GPS assente ovunque | I-16, I-27 verdi |
| **F7 — H8** | `packages/ledger` (pubblico), `m4tr1x-issuer` (privato), repliche su nodi ed edge, spese e mance (split) dall'app con K2. **Solo rete di prova**, coni di prova, buoni semplici | Doppia spesa concorrente → una sola riga; registro manomesso → replica lo rifiuta; equivocazione → allarme; trasferimento utente → utente rifiutato | I-09…I-14, I-34 verdi |
| **F7b — Buoni ciechi** | Buoni RSA Blind Signatures (RFC 9474) + flusso di riconciliazione dei bonifici. Prima della vendita reale | Il riscatto non è collegabile al bonifico; buono riusato → rifiutato | I-35 verde; parere legale ricevuto prima di accendere la vendita |
| **F8 — Aggiornamenti e release** | `tools/release`, `release.json` firmato offline, updater nell'app e nel nodo, build doppia in CI | Firma falsa → rifiutata; versione vecchia → rifiutata; due build → stessi hash (dove previsto) | I-21, I-22, I-24 verdi |
| **F9 — Mobile** | Guscio Tauri Android (poi iOS) con tutte le funzioni F4–F8 | Flusso completo su telefono reale; APK firmato K12 + manifest K11 | Tutte le invarianti dell'app verdi su Android |
| **F10 — Distributori** | Pagamenti H8 all'edge, buoni offline con tetto | Edge senza rete accetta fino al tetto e rifiuta oltre; riconciliazione corretta | I-32 verde |
| **F11 — Lancio controllato** | Cerimonia delle chiavi (KEYS.md), deploy di 3 ancore + 2 hub + 1 edge reali, audit esterno se possibile | Checklist di lancio; rete principale accesa | Decisione esplicita del fondatore |

**Perché questo ordine.** Prima ciò da cui dipende tutto (CI, chiavi, protocollo), poi il nodo, poi la directory che sostituisce l'head. Il prototipo Tauri + Arti (F4a) viene **prima** dell'app vera perché è la scommessa più rischiosa: se fallisce, conviene saperlo dopo poche giornate, non dopo settimane. H8 arriva dopo i contenuti perché, senza contenuti, non c'è niente da pagare, e perché dipende dalle decisioni legali (OPEN_QUESTIONS Q1, Q2).

## 4. Regole di lavoro (ripetute qui perché valgono in ogni fase)

- Mai segreti nei commit: chiavi, `.env`, database, chiavi onion, IP, email personali (gitleaks in CI e in pre-commit).
- Email dei commit: solo l'indirizzo `noreply` di GitHub; autore `H8db0y`.
- Mai force-push né operazioni distruttive senza chiedere.
- Messaggi dei commit di sicurezza in forma neutra (si descrive la correzione, non l'attacco).
- Se una decisione presa si rivela sbagliata durante la costruzione: ci si ferma, si scrive il problema, si chiede.
