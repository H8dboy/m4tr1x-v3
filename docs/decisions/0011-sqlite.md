# ADR-0011 — SQLite ovunque; via Postgres e l'head

**Contesto.** Esistono due head (uno incorporato nel nodo con SQLite, uno separato con Postgres) con formati di blocco incompatibili (M17). La decisione è "niente head centrale".

**Decisione.** SQLite (better-sqlite3) in nodo, edge, hub, ancore ed emittente. Il repo `m4tr1x-head` non si porta avanti.

**Conseguenze.**
- (+) Un solo database da conoscere; niente server di database da proteggere; backup = copia di un file.
- (−) L'emittente scrive da un solo processo: basta per il volume previsto (migliaia di righe al giorno). Se servisse di più, nuovo ADR.
