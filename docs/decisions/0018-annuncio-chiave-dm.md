# ADR-0018 — Messaggi privati: chiave DM separata, annunciata dall'identità

**Stato:** proposta il 25/10/2026, per F5 (da confermare dal fondatore al merge). Precisa ARCHITECTURE §7 e KEYS.md (K1d).

**Kind scelto.** Non si riusa il kind 10050 di NIP-17 (lista dei relay preferiti per i DM): con un significato diverso causerebbe confusione e incompatibilità con altri client. **10477** è un kind sostituibile nostro, non assegnato da alcun NIP al 25/10/2026 (da riverificare prima del lancio). Il tag `["relay", …]` ha lo stesso senso di NIP-17.

**Contesto.** I messaggi privati seguono NIP-17 (messaggio kind 14 → sigillo kind 13 → busta kind 1059, NIP-44 v2). Il sigillo è firmato dal mittente: se la chiave è l'identità (K1), chi ruba K1 legge anche i DM vecchi che ha conservato. Serve una chiave DM separata e ruotabile (K1d, `m4tr1x/dm/v<n>`), ma allora il destinatario deve poter sapere **di chi** è una chiave DM.

**Decisione.**
1. Il sigillo è firmato dalla **chiave DM** (ruolo `Dm`), non dall'identità. L'identità non cifra né firma mai un DM.
2. L'identità annuncia le sue chiavi DM con un evento **kind 10477** (sostituibile): un tag `["dm", <pubkey DM>, <prova>]` per chiave (la corrente e, finché servono, le precedenti), più i tag `["relay", …]` dei relay per i DM. La **prova di possesso** è la firma Schnorr della chiave DM sul testo `m4tr1x/dm-annuncio/v1:<rete>:<pubkey identità>:<pubkey DM>`: senza di essa, chiunque potrebbe annunciare la chiave DM di un altro e farsi attribuire i suoi messaggi.
3. Un messaggio aperto è attribuito a un'identità **solo** se esiste, per la rete giusta, l'annuncio più recente di quell'identità che contiene la chiave DM del sigillo con prova valida. Altrimenti il mittente è **sconosciuto**: il messaggio si può mostrare come "da un mittente sconosciuto", mai a nome di qualcuno.
4. Le buste hanno solo i tag `p` (destinatario, serve al relay) e `net` (il nodo ne esige uno). Una busta per il destinatario e una per il mittente (la sua copia), con chiavi usa e getta diverse.
5. I kind 4, 13 e 14 non viaggiano mai su un relay: nodo, `verifyEvent` (JS) e `verifica` (Rust) li rifiutano. Il sigillo (13) esiste solo dentro la busta cifrata.

**Limite dichiarato: nessuna forward secrecy in v3.** Se una chiave DM viene rubata, chi ha conservato le buste vecchie cifrate per quella chiave le può leggere. La rotazione della chiave DM limita il danno ai messaggi di quel periodo, non lo elimina. Un protocollo con ratchet (MLS, o simile a Signal) è un progetto a sé, **dopo la v3**; questa scelta non lo impedisce (le buste sono un trasporto, il contenuto si può cambiare formato).

**Rischi noti, non risolti qui.**
- Il relay vede il destinatario (tag `p`) e la dimensione approssimata: tabella in ARCHITECTURE §7.
- Le quote per autore del relay (I-41) non limitano le buste, perché ogni busta ha una chiave nuova: un flood di buste verso un destinatario si ferma solo con il tetto globale e la dimensione massima. Da affrontare quando si scrive il relay per DM (limite per `p`).

**Conseguenze.**
- (+) Rubare l'identità non basta per leggere i DM; ruotare la chiave DM non cambia l'identità.
- (+) Nessun mittente falso: l'attribuzione richiede una prova.
- (−) Chi scrive deve prima trovare l'annuncio del destinatario; senza, non può scrivergli.
- Invarianti nuove: **I-60** (attribuzione), **I-61** (buste forgiate scartate). Attivate: **I-19**, **I-20**.
