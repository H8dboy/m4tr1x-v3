# ADR-0013 — Tor: `tor` sui server, Arti nelle app

**Contesto.** I server devono *ospitare* servizi onion; le app devono solo *collegarsi* a servizi onion. In Arti l'ospitare servizi onion è ancora sperimentale; il client è maturo.

**Decisione.** Sui server il demone `tor` (C), gestito dal nodo (riuso di `tor_node.js`). Nelle app Arti incorporato, solo come client. Bridge (obfs4/Snowflake) configurabili nell'app per i Paesi che bloccano Tor.

**Conseguenze.**
- (+) Ogni lato usa la parte più matura di Tor.
- (−) Due implementazioni di Tor da tenere aggiornate. Quando Arti sarà stabile anche per ospitare, i server potranno passare ad Arti (nuovo ADR).
