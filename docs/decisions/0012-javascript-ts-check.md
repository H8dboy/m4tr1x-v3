# ADR-0012 — Server in JavaScript con `@ts-check`

**Contesto.** Il codice da riusare (relay, Blossom, NIP-17, Tor, pipeline video) è JavaScript. Il fondatore conosce JavaScript; TypeScript aggiunge un passo di compilazione; Rust allungherebbe molto i tempi.

**Decisione.** Node.js LTS, JavaScript con `// @ts-check` e tipi in JSDoc, controllati in CI con `tsc --noEmit`. Rust solo nel guscio dell'app.

**Conseguenze.**
- (+) Si riusa codice già testato; gli errori di tipo si trovano in CI senza cambiare linguaggio.
- (−) Tipi meno rigorosi di TypeScript vero. Si può passare a TypeScript più avanti, un file alla volta.
