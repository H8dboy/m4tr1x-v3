# ADR-0016 — Interfaccia di trasporto unica: Arti dove regge, Tor classico su Windows

> **Aggiornamento del 21/10/2026 (decisione del fondatore) — cambia il punto 2.** Sul **desktop**
> (Windows, Linux, macOS) l'app usa **`TorProcessTransport`** (il `tor` classico come processo
> accanto all'app); **`ArtiTransport` solo su mobile** (Android, iOS), dove un processo separato è
> difficile. Motivo: con `tor` fuori dall'app la regola I-01 diventa netta e si prova in modo severo
> (l'app parla SOLO con 127.0.0.1, verso le porte del suo tor; qualsiasi altro pacchetto è una
> violazione), mentre con Arti dentro l'app l'app parla direttamente con i relay e la prova dovrebbe
> confrontare ogni destinazione con il consenso (più fragile). Non serve più un `ArtiTransport` per
> Linux in F4. **Costo:** un binario `tor` con hash fissato da mantenere anche per Linux e macOS
> (Tor Expert Bundle ufficiale), non solo per Windows. **Conseguenze:** le regole di
> `TorProcessTransport` qui sotto e I-37 valgono per tutto il desktop; il lock del binario diventa
> uno per piattaforma (oggi c'è solo `app/tor-windows.lock.json`); il punto 4 diventa "il giorno in
> cui Arti regge sul desktop, nuovo ADR". La storia sotto resta com'era.

**Stato:** accettata il 15/10/2026 (decisione del fondatore). Completa l'ADR-0002.

**Contesto.** Il prototipo F4a (rapporto in `docs/spikes/F4a-REPORT.md`) ha mostrato che Arti 0.47 / arti 2.7.0 su **Windows** si blocca al download del consenso: succede sulla rete dell'officina, su hotspot, con il controllo dei permessi disattivato e con l'eccezione di Defender per processo e cartella. Su **Linux**, dalla stessa rete, parte in 11 s. Il `tor` classico (C) su Windows parte in 9–31 s. L'ispezione di rete di Defender (NIS) non è stata esclusa: si disattiva solo per tutto il PC, e non lo si è fatto. Su Android la misura manca (nessun telefono disponibile).

**Decisione.**
1. Nel lato Rust dell'app esiste **un solo modulo di rete**, `transport`, con **un'interfaccia unica**. Il resto dell'app (protocollo, directory, blob, aggiornamenti) usa solo questa interfaccia e non sa quale Tor c'è sotto.
   ```rust
   trait Transport {
       async fn bootstrap(&self) -> Result<BootstrapInfo>;           // avvio + stato (0–100%)
       async fn connect_onion(&self, host: &OnionV3, port: u16, isolation: Isolation) -> Result<Stream>;
       fn status(&self) -> TransportStatus;                            // per l'interfaccia e i test
       async fn shutdown(&self);
   }
   ```
   - `connect_onion` accetta **solo** indirizzi onion v3 già validati (tipo `OnionV3`): non esiste un metodo per collegarsi a un host in chiaro.
   - `Isolation` decide se due richieste possono condividere il circuito (es. identità diverse = circuiti diversi).
2. **Due implementazioni**, scelte in compilazione per piattaforma:
   - `ArtiTransport` — Linux, macOS, Android, iOS (Android/iOS da confermare con la misura F4a sul telefono).
   - `TorProcessTransport` — **Windows**: l'app avvia il `tor` classico incluso nel pacchetto e ci parla via SOCKS5 e porta di controllo.
3. Un'opzione di configurazione permette di forzare l'altra implementazione **solo nei test e nei prototipi**, non nell'app distribuita.
4. Il giorno in cui Arti regge su Windows, si cambia **una riga** di selezione per piattaforma e si toglie il `tor` dal pacchetto Windows (nuovo ADR).

**Regole per `TorProcessTransport` (Windows)** — tutte con test:
- Il binario `tor.exe` viene dal Tor Expert Bundle ufficiale, con **hash SHA-256 fissato nel repo** e verificato in build; entra nel pacchetto firmato (il suo hash finisce anche nel `release.json`).
- `SocksPort` e `ControlPort` solo su `127.0.0.1`, porte casuali scelte all'avvio; `SocksPort` con **`OnionTrafficOnly`**; credenziali SOCKS casuali per sessione con `IsolateSOCKSAuth`; porta di controllo con **cookie**; `__OwningControllerProcess` legato al processo dell'app, così `tor` muore con l'app.
- **Correzione del 16/10/2026 (decisione del fondatore, opzione A).** Questa ADR prometteva "autenticazione SOCKS obbligatoria, così un altro programma del PC non può usare il nostro Tor". **Non è vero:** `tor` C (0.4.9.11) non ha un'opzione che renda obbligatoria l'autenticazione SOCKS, e una prova dal vivo l'ha confermato (handshake senza credenziali accettato). Le credenziali **isolano** i circuiti, non chiudono la porta. Garanzie reali: (1) con `OnionTrafficOnly` chi usa il nostro SOCKS raggiunge **solo onion**, mai host in chiaro (provato: dominio e IP rifiutati con risposta 2); (2) senza le nostre credenziali non condivide i nostri circuiti; (3) solo 127.0.0.1, porte casuali. Rischio residuo: un programma già presente sul PC può usare il nostro `tor` per raggiungere onion; un programma così ha già in mano l'utente ed è fuori dal modello delle minacce. Un controllo d'accesso vero arriverà solo con Arti su Windows (cambio di una riga, punto 4). I-37 riscritta di conseguenza.
- `DataDirectory` nella cartella dati dell'app, mai condivisa con altri Tor.
- Nessun servizio onion ospitato dall'app (l'app è solo client).

**Conseguenze.**
- (+) Si va avanti subito su tutte le piattaforme; il cambio futuro tocca un solo modulo.
- (+) L'invariante I-01 ("nessuna richiesta fuori da Tor") si controlla in un punto solo: nessun codice fuori da `transport` apre connessioni.
- (−) Su Windows due cose in più da mantenere: il binario `tor` (aggiornamenti di sicurezza da seguire) e la gestione del processo. Pacchetto Windows più grande di ~10 MB.
- (−) Due implementazioni da testare: la stessa suite di test del trasporto gira su entrambe.

**Invarianti nuove** (in `SECURITY_INVARIANTS.md`): I-36 (solo il crate `app/transport` apre connessioni, trasporto unico), I-37 (il Tor di Windows serve solo onion e non presta i circuiti dell'app; corretta il 16/10/2026).

**Struttura (16/10/2026).** `transport` è un **crate separato** (`app/transport/`) e non una cartella del guscio: gli altri crate non dipendono da librerie di rete, quindi è il compilatore a impedire connessioni fuori dal trasporto.
