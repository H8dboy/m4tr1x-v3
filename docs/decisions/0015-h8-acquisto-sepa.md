# ADR-0015 — H8: acquisto con bonifico SEPA manuale, buoni

**Contesto.** Decisione del fondatore (15/10/2026, Q2): H8 si compra con bonifico SEPA manuale a 1,25 € per H8, solo dopo un parere legale (MiCA). Il bonifico rivela nome e IBAN di chi paga.

**Decisione.**
- Nessun fornitore di pagamenti: l'emittente riceve i bonifici sul conto H8-Group e li riconcilia a mano.
- Il bonifico compra un **buono** (codice), non un accredito diretto: il buono si riscatta più tardi da qualsiasi wallet, via Tor (Q17 opzione B).
- Prima della vendita reale, i buoni diventano **a firma cieca** (RSA Blind Signatures, RFC 9474), così l'emittente non può collegare il bonifico al conto che riscatta (Q17 opzione C, invariante I-35).
- Il conio corrispondente ai buoni venduti avviene **a lotti, offline, con K9**.
- Nessuna vendita reale prima del parere legale; fino ad allora solo rete di prova. Dal sito si toglie la parola "riserva".

**Conseguenze.**
- (+) Nessuna chiave API di pagamento online (K14 sparisce); il denaro non tocca i server.
- (+) Con i buoni ciechi, chi compra non è collegabile a ciò che fa con gli H8.
- (−) Lavoro manuale per ogni acquisto; tempi di accredito di 1–2 giorni lavorativi.
- (−) Fino ai buoni ciechi, l'emittente può collegare acquirente e conto: va detto chiaramente nell'app.
