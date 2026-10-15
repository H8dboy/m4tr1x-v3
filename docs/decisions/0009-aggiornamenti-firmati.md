# ADR-0009 — Aggiornamenti firmati con manifest e anti-rollback

**Contesto.** Oggi le app non controllano gli aggiornamenti: il 15/10, per avvisare chi aveva versioni vulnerabili, l'unico canale era il sito. Le release Android di giugno erano firmate con la chiave di debug.

**Decisione.** Ogni release ha un `release.json` (versione, `seq`, hash dei pacchetti, versione minima) firmato offline con K11 (Ed25519, formato minisign). App e nodo incorporano la chiave pubblica, controllano via Tor, verificano firma e hash, rifiutano versioni o `seq` più bassi (I-21, I-22). La CI non pubblica nulla senza manifest firmato (I-23). Su Android serve anche la firma K12.

**Conseguenze.**
- (+) Si possono avvisare e aggiornare gli utenti; nessuno può spingere versioni vecchie e bucate.
- (+) Indipendente dagli store.
- (−) Il fondatore firma a mano ogni release (voluto: la chiave non sta in CI).
- (−) iOS senza App Store: l'app avvisa, ma l'installazione resta manuale (sideload).
