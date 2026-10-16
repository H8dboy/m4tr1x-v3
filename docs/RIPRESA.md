# Nota di ripresa — 16/10/2026

> Fonte di verità: codice e test.

## Stato
- Repo privato `H8dboy/m4tr1x-v3`. Test locali (Node 22.23.3): **135 verdi, 0 falliti**, 21 regole attive, 0 divergenze doc↔codice (`npm run report`).
- Sabotaggio completo: **43/43 prese** (`docs/generated/sabotage.json`). Corretta una prova invecchiata (I-38 purge: il testo cercato era cambiato con I-41).
- Fasi chiuse: **F0, F1, F2, F3**. F4a aperta (serve telefono Android; Arti non regge su Windows → ADR-0016).

## Fatto il 16/10
1. KEYS.md e report: K5/K13 nel vault, passphrase obbligatoria, compromesso scritto; debolezze 3–5 e 13 chiuse nel report.
2. **Musica e topic dentro v3.0**: musica = Blossom + HLS audio in F6 (I-16 estesa all'audio); topic = NIP-72 in nuova fase **F6b** (proposta I-42). Stima totale ~45–59 giornate.
3. **Q18 deciso**: procedura DSA notice and action scritta e approvata **prima di iniziare F6** (precondizione in MIGRATION_PLAN). Q19 (moderazione topic) aperta.
4. **Bozza** `docs/design/PEZZI_MANCANTI.md`: monitoraggio, registrazione creator (mappa minima e cifrata con K17, nessuno storico, proposta I-50), shop fisico (K16, indirizzi cancellati dopo 30 gg), boost (ordinamento nell'app, "Promosso", tetto posizioni). **Da approvare.**

## Prossimo
- OK del fondatore sulla bozza PEZZI_MANCANTI (tabella in fondo) → numeri I-42…I-50, K16, K17 entrano in SECURITY_INVARIANTS/KEYS.
- Scrivere la procedura DSA (precondizione di F6).
- Poi **F4** (app desktop con `transport`) o **F5** (DM), a scelta del fondatore.

## Fuori dal repo, ancora aperto
- Sito nderja.com: correggere "identità ML-DSA", "codice pubblico", download Kalendae rotto.
- Telefono Android per F4a.
- Repo v2: rotazione di `h8identity.enc` mai fatta.
