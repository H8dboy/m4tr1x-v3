# Nota di ripresa — 15/10/2026, ore ~16

> Scritta per riprendere il lavoro dopo il reset del limite. Fonte di verità: codice e test.

## Stato al momento della nota
- Repo privato `H8dboy/m4tr1x-v3`, CI verde all'ultimo commit pubblicato prima di questo.
- Test in locale (Node 22.23.3, `npm run test:pinned`): **135 verdi, 0 falliti**, 1 saltato (prova via Tor vero: serve `M4TR1X_TOR_BIN`), 20 regole in attesa.
- Fasi chiuse: **F0, F1, F2, F3**. F4a aperta (manca telefono Android; Arti non regge su Windows → ADR-0016).

## Fatto in questa sessione (dopo il report)
1. **Tag di prova del cancello release ML-DSA in CI**: prima prova rifiutata per il motivo SBAGLIATO (modulo mancante, il job non installava le dipendenze) → corretto il workflow → seconda prova rifiutata per il motivo giusto ("chiave di release assente: cerimonia non fatta"). Tag cancellati.
2. **Debolezze 3–5 del report chiuse (lato server)**, con test e prove di sabotaggio:
   - **K5 e K13 non più in chiaro**: vault `secrets.vault.json` (scrypt N=2^17 + AES-GCM). Il nodo **non parte senza passphrase** (`keyPassphrase`, `keyPassphraseFile` o `M4TR1X_KEY_PASSPHRASE`). File in chiaro vecchi sigillati, sovrascritti e cancellati. Test: `inv-06b-node-secrets`.
   - **Spazio del relay limitato** (nuova regola **I-41**): dimensione massima per evento (64 KB), quota per autore (10.000 eventi / 64 MB), tetto globale (1 M eventi / 2 GB); autori fidati (`uploaders`) esenti; la sincronizzazione dai peer rispetta gli stessi limiti. Test: `inv-41-relay-space`.
   - **Budget di chiavi nuove per ancora-peer** (500/ora di default): un'ancora compromessa non può più riempire le altre. Test aggiunto in `inv-39-anchor-limits`.
   - Prove di sabotaggio aggiunte in `tools/sabotage/mutations.js` (ora 43): le 6 nuove eseguite una per una → **tutte prese**. La campagna completa con salvataggio NON è stata rifatta.

## Da fare alla ripresa (in ordine)
1. `npm run sabotage` (campagna completa, salva l'esito) e poi `npm run report` (rigenera `docs/STATO_M4TR1X.md`); controllare che il report segni le debolezze 3–5 come chiuse e **aggiornare a mano la sezione 7** (debolezze) e la sezione 3 (K5/K13 ora in vault, non più "in chiaro").
2. Aggiornare `docs/KEYS.md` (K5, K13 nel vault, passphrase obbligatoria) e il compromesso: chi ha la macchina E la passphrase ha le chiavi; una copia della sola cartella dati no.
3. **Musica e topic dentro la v3.0** (decisione del fondatore): musica = Blossom + HLS audio (F6); topic = comunità NIP-72. Aggiornare ARCHITECTURE (tabella protocolli), MIGRATION_PLAN (fasi), OPEN_QUESTIONS, report sezione 1.
4. **Progettare i pezzi mancanti** indicati dal fondatore (documenti, poi fasi): monitoraggio (senza IP, metriche solo admin), registrazione dei creator (mappa privata pubkey→conto presso l'emittente, revocabile), shop fisico (ordini e indirizzi di spedizione = dati personali sensibili: cifrati per lo shop, conservazione minima), progetto del boost (spesa H8 nel registro che cita l'id dell'evento; come lo usano i client per ordinare il feed senza server centrale).
5. Poi **F4** (app desktop con modulo `transport`) o **F5** (DM), a scelta del fondatore.

## Fuori dal repo, ancora aperto
- Sito nderja.com: correggere "identità ML-DSA", "codice pubblico", download Kalendae rotto.
- Telefono Android per F4a.
- Repo v2: rotazione di `h8identity.enc` mai fatta.
