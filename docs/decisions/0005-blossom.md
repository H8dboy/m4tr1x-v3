# ADR-0005 — Media: Blossom, upload a pezzi

**Contesto.** I file devono poter essere serviti da qualsiasi nodo o edge senza fidarsi di chi li serve. Su Tor gli upload lunghi cadono spesso.

**Decisione.** Blossom: ogni file è un blob identificato dal suo SHA-256. I video si caricano in **pezzi da 4 MB**, ognuno un blob; un evento firmato dall'autore elenca i pezzi. Chi riceve verifica sempre l'hash (I-17).

**Conseguenze.**
- (+) Cache ovunque senza fiducia; upload che riprendono da dove si sono interrotti.
- (+) Nessun protocollo in più per l'upload ripristinabile (niente tus o simili).
- (−) Un evento in più per video; l'hub deve ricomporre i pezzi.
