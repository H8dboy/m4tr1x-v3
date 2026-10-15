# ADR-0016 — Interfaccia di trasporto unica: Arti dove regge, Tor classico su Windows

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
- `SocksPort` e `ControlPort` solo su `127.0.0.1`, porte casuali scelte all'avvio; **autenticazione SOCKS obbligatoria** con credenziali casuali per sessione (e `IsolateSOCKSAuth`), così un altro programma del PC non può usare il nostro Tor; porta di controllo con **cookie**; `__OwningControllerProcess` legato al processo dell'app, così `tor` muore con l'app.
- `DataDirectory` nella cartella dati dell'app, mai condivisa con altri Tor.
- Nessun servizio onion ospitato dall'app (l'app è solo client).

**Conseguenze.**
- (+) Si va avanti subito su tutte le piattaforme; il cambio futuro tocca un solo modulo.
- (+) L'invariante I-01 ("nessuna richiesta fuori da Tor") si controlla in un punto solo: nessun codice fuori da `transport` apre connessioni.
- (−) Su Windows due cose in più da mantenere: il binario `tor` (aggiornamenti di sicurezza da seguire) e la gestione del processo. Pacchetto Windows più grande di ~10 MB.
- (−) Due implementazioni da testare: la stessa suite di test del trasporto gira su entrambe.

**Invarianti nuove** (in `SECURITY_INVARIANTS.md`): I-36 (solo `transport` apre connessioni, trasporto unico), I-37 (il Tor di Windows non è usabile da altri processi).
