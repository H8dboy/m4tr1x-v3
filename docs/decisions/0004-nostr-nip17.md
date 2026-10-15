# ADR-0004 — Post e DM: Nostr + NIP-17

**Contesto.** Servono post firmati dall'utente (non da un server) e messaggi privati che non rivelino chi scrive a chi. Il 15/10 è emerso che i DM attuali (kind 14 firmati) rivelavano mittente e destinatario; è stato portato NIP-17.

**Decisione.** Nostr (NIP-01) per post, profili, reazioni e follow; NIP-17 (gift wrap NIP-59 + NIP-44) per i DM; chiave DM separata e ruotabile (K1d). Si riparte da zero, quindi **NIP-04 si toglie del tutto** (nemmeno in lettura).

**Conseguenze.**
- (+) Identità indipendente dai server; un protocollo per tutto ciò che è "evento".
- (−) Niente forward secrecy (dichiarato nel THREAT_MODEL).
- (−) Il relay del destinatario vede a chi è diretta una busta.
