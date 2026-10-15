# ADR-0008 — Chiavi separate ricavate da un seme

**Contesto.** Oggi la chiave Nostr è ricavata dalla chiave H8 (`deriveSigningKey`) e il tag `h8` con l'indirizzo del wallet finisce su ogni post: identità e denaro sono legati e pubblici. Il nodo salva in un file la password della propria identità (M2).

**Decisione.** Un seme di 24 parole; da esso, con etichette diverse (HKDF), chiavi indipendenti per identità, DM, revoca e wallet. Le chiavi di sistema (nodo, ancora, hub, emittente, release, keystore, admin) sono generate a parte e non derivano da nulla di condiviso. Ogni firma ha un'etichetta di dominio (I-08). Dettagli in `KEYS.md`.

**Aggiornamento 15/10/2026 (ADR-0017):** dal seme si ricavano chiavi di due famiglie: Schnorr (identità, DM, revoca) e ML-DSA-65 (wallet).

**Conseguenze.**
- (+) Un solo backup per l'utente; il furto di una chiave di ruolo non dà le altre.
- (−) Il seme è un punto unico per l'utente: per questo non resta mai in digitale.
