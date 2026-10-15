# ADR-0011 — SQLite ovunque; via Postgres e l'head

**Contesto.** Esistono due head (uno incorporato nel nodo con SQLite, uno separato con Postgres) con formati di blocco incompatibili (M17). La decisione è "niente head centrale".

**Decisione.** SQLite (better-sqlite3) in nodo, edge, hub, ancore ed emittente. Il repo `m4tr1x-head` non si porta avanti.

**Conseguenze.**
- (+) Un solo database da conoscere; niente server di database da proteggere; backup = copia di un file.
- (−) L'emittente scrive da un solo processo: basta per il volume previsto (migliaia di righe al giorno). Se servisse di più, nuovo ADR.

**Aggiornamento 15/10/2026 (F2).** Si usa SQLite **incluso in Node** (`node:sqlite`) invece di `better-sqlite3`: nessuna dipendenza nativa da compilare (era la parte che si rompeva a ogni aggiornamento di Electron). In Node 22 `node:sqlite` è sperimentale, quindi (decisione del fondatore) la **versione di Node è fissata in modo esatto** (`.node-version`, ora 22.23.3) in CI e nei nodi: il nodo non parte con un'altra versione, e un test controlla le funzioni SQLite usate. Aggiornare Node = cambiare `.node-version` e far passare quei test.
