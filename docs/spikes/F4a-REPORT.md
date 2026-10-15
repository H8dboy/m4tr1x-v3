# F4a — Prototipo Tauri 2 + Arti: rapporto intermedio (15/10/2026)

> Stato: **NON concluso.** Il criterio decisivo (telefono Android reale) non è misurabile: il fondatore al momento non ha un telefono Android.
> Codice del prototipo: `spikes/f4a-arti/` (usa e getta, non è codice di prodotto).

## Cosa è stato costruito

- App Tauri 2 minima; tutta la rete passa da `arti-client 0.47.0` (TLS rustls + ring, SQLite incluso, client per servizi onion).
- Cinque misure: avvio di Tor, collegamento, download di 10 MB con verifica SHA-256, 10 collegamenti su circuiti nuovi, prova di 30 minuti. Modalità `--bench` da riga di comando per misurare senza interfaccia.
- Server di prova: servizio onion ospitato con `tor` 0.4.9.11 (Tor Expert Bundle ufficiale, hash verificato), file da 10 MB con hash noto. Indirizzo fuori dal repo.

## Risultati

| Prova | Esito |
|---|---|
| Compilazione desktop Windows | **OK** — eseguibile 12,9 MB con Tor incluso |
| Compilazione della libreria per Android (aarch64) | **OK** — Arti si compila per Android |
| Pacchetto APK (passo Gradle) | **Non completato** — lo script Gradle generato cerca la CLI di Tauri nel progetto; problema di configurazione, non di Arti |
| Arti (prototipo) su Windows, rete officina | **Bloccato** al download del consenso (canali Tor aperti, ~720 KB ricevuti, poi stallo senza errori) |
| `arti` CLI ufficiale 2.7.0 su Windows, rete officina | **Bloccato** nello stesso punto |
| `arti` CLI su Windows, hotspot del telefono | **Bloccato** nello stesso punto |
| `arti` CLI su Windows senza controllo permessi | **Bloccato** |
| `tor` (C) su Windows, rete officina | **OK in 9 s** |
| `tor` (C) su Windows, hotspot | **OK in 31 s** |
| `arti` CLI 2.7.0 su **Linux** (container), rete officina | **OK in 11 s** |
| Compressione disattivata (prototipo, Windows) | **Bloccato** (esclusa la compressione come causa) |

## Conclusione provvisoria

- Il problema **non è la rete** (stesso comportamento in officina e su hotspot; `tor` C funziona su entrambe) e **non è il codice del prototipo** (la CLI ufficiale si blocca allo stesso modo).
- È un comportamento di **Arti 0.47 / 2.7 su Windows** (su Linux, stessa rete, funziona in 11 s). Causa non ancora individuata.
- Per **Android** (sistema basato su Linux) il segnale è **positivo ma non verificato**: Arti si compila per Android e funziona su Linux, ma nessuna misura su un telefono reale è stata fatta.

## Conseguenze per l'ADR-0002

- **Android/iOS:** decisione rinviata alla misura su telefono reale (criteri invariati in MIGRATION_PLAN F4a).
- **Desktop Windows:** con Arti 0.47 oggi **non** regge. Opzioni: (a) piano B solo su Windows: `tor` C come processo accanto all'app, controllato dal lato Rust (sul desktop si può, è ciò che faceva il vecchio Electron); (b) riprovare con versioni successive di Arti e segnalare il problema al progetto Arti con i log raccolti. Raccomandazione: (a) per non bloccarsi, (b) in parallelo.
- **Niente decisione definitiva** finché non c'è la misura sul telefono.

## Cosa resta

1. Telefono Android (anche in prestito) per i 5 criteri.
2. Sistemare il passo Gradle (installare `tauri-cli` via cargo o aggiungere la CLI come dipendenza del prototipo).
3. Raccogliere i log di Arti su Windows e aprire una segnalazione al progetto Arti.
