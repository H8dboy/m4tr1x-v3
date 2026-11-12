# ADR-0005 — Media: Blossom, upload a pezzi

**Contesto.** I file devono poter essere serviti da qualsiasi nodo o edge senza fidarsi di chi li serve. Su Tor gli upload lunghi cadono spesso.

**Decisione.** Blossom: ogni file è un blob identificato dal suo SHA-256. I video si caricano in **pezzi da 4 MB**, ognuno un blob; un evento firmato dall'autore elenca i pezzi. Chi riceve verifica sempre l'hash (I-17).

**Conseguenze.**
- (+) Cache ovunque senza fiducia; upload che riprendono da dove si sono interrotti.
- (+) Nessun protocollo in più per l'upload ripristinabile (niente tus o simili).
- (−) Un evento in più per video; l'hub deve ricomporre i pezzi.

**Stato (12/11/2026).** Fatto nell'app e nel nodo, senza evento in più: la scheda del file (kind 1063) elenca i pezzi in ordine con tag `pezzo` (`x` = hash del file intero, `size`, tanti `pezzo` quanti `ceil(size / 4 MiB)`, nessuno se il file sta in un pezzo); il nodo accetta blob al massimo di un pezzo (`maxBlobBytes` predefinito 4 MiB). Caricamento: `HEAD /blob/<sha>` per saltare i pezzi già arrivati, un'autorizzazione firmata per pezzo (firmata solo quando quel pezzo parte). Scarico: pezzo per pezzo dalle fonti in ordine (nodo dei file, poi nodo), hash del pezzo e poi hash e dimensione del file intero (I-17, I-64). Limite di un file: 1 GiB (256 pezzi), intero in memoria sul computer. Gli hub HLS (ADR-0006) restano da fare.
