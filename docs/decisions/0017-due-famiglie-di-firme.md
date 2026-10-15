# ADR-0017 — Due famiglie di firme, in un solo pacchetto crittografico

**Stato:** accettata il 15/10/2026 (decisione del fondatore). Modifica ADR-0007, ADR-0008, ADR-0009 e KEYS.md.

**Contesto.** Fino a F3 tutte le chiavi dell'utente (identità e wallet) erano Schnorr secp256k1 e la chiave di release era Ed25519 (minisign). ML-DSA (post-quantum) era previsto solo per l'emittente H8. Conseguenza: ogni spesa H8 portava una firma ML-DSA (emittente) e una Schnorr (wallet), e con un computer quantistico sufficiente bastava falsificare la seconda per spendere i saldi altrui. Inoltre nderja.com presentava l'identità come "ML-DSA-65", cosa non vera in v3 (Nostr impone Schnorr).

**Decisione.** Una regola semplice:

| Famiglia | Ruoli | Perché |
|---|---|---|
| **Schnorr secp256k1** | tutto il social: identità (K1), DM (K1d), revoca (K1r), nodo (K5), ancora (K7), hub (K8) | è ciò che Nostr richiede; firme piccole (64 byte) su moltissimi eventi |
| **ML-DSA-65** | tutto ciò che vale: wallet (K2), radice emittente (K9), emittente operativo (K10), release (K11) | resistente ai computer quantistici dove un falso costa denaro o compromette tutti gli utenti; poche firme, il peso (3309 byte) non conta |

Eccezioni, chiuse nel loro recinto:
- **Ed25519** lo gestisce solo Tor (chiave del servizio onion, K6): nessun codice M4TR1X lo usa.
- **RSA** solo nel modulo dei buoni ciechi (K15, RFC 9474, fase F7b): nessun altro modulo lo usa.
- **Firma dell'APK (K12)**: la impone Android (RSA/EC). Non è una garanzia su cui contiamo: l'app accetta un aggiornamento solo se il manifest è firmato ML-DSA con K11.

Un solo pacchetto (`packages/keys`) conosce la famiglia di ogni ruolo: genera, deriva, firma e rifiuta l'uso incrociato. `packages/protocol` sa quale famiglia verifica ogni dominio: una firma Schnorr non viene mai accettata per un dominio di valore, né una ML-DSA per un dominio social.

**Conseguenze.**
- (+) Il registro H8 è resistente ai computer quantistici per intero (utente + emittente); anche le release.
- (+) Una regola da ricordare invece di un elenco di casi.
- (−) Firme e chiavi del wallet più pesanti: ogni spesa ~3,3 KB di firma in più; chiave pubblica 1952 byte. Accettabile per il volume previsto.
- (−) La chiave di release non è più compatibile con lo strumento `minisign`: serve il nostro strumento di firma offline (`tools/release`).
- (−) Va corretto il testo di nderja.com: "identità Nostr (Schnorr); registro H8 e aggiornamenti post-quantum (ML-DSA-65)".
- Invariante nuova: **I-40**.
