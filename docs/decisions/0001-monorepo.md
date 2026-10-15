# ADR-0001 — Monorepo

**Contesto.** Oggi 4 repo (node, electron, android, head) con codice duplicato: electron ha una copia del server del nodo rimasta indietro (relay vulnerabile, firma Schnorr rotta: trovati il 15/10). Le correzioni andavano fatte due volte e una volta su due ne mancava un pezzo. Uno sviluppatore solo.

**Decisione.** Un solo repo `m4tr1x` con: `packages/` (codice condiviso: protocollo, firme, **verifica** del registro H8), `node/` (programma server), `app/` (Tauri 2 + interfaccia), `tests/invariants/`, `docs/`.
**Aggiornamento 15/10/2026 (decisione del fondatore, Q3):** l'emittente H8 sta in un repo **privato separato** `m4tr1x-issuer`, che usa `packages/ledger` del monorepo come dipendenza. Il monorepo è privato fino al lancio, poi pubblico.

**Conseguenze.**
- (+) Una modifica al protocollo aggiorna server e app nello stesso commit: nessuna copia resta indietro.
- (+) Una sola CI, un solo gitleaks, una sola lista di dipendenze vietate.
- (−) Repo più grande; la CI deve capire cosa ricompilare (filtri per cartella).
- (−) Due repo invece di uno per la parte H8: una modifica al formato delle righe va rilasciata prima in `packages/ledger` e poi adottata dall'emittente. Il test di compatibilità gira in entrambi.
