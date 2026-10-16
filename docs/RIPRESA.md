# Nota di ripresa — 16/10/2026

> Fonte di verità: codice e test.

## Stato
- Repo privato `H8dboy/m4tr1x-v3`. Test locali (Node 22.23.3): **135 verdi, 0 falliti**, 21 regole attive, 0 divergenze doc↔codice (`npm run report`).
- Sabotaggio completo: **48/48 prese** (16/10, con tor vero); prima 43/43 (`docs/generated/sabotage.json`). Corretta una prova invecchiata (I-38 purge: il testo cercato era cambiato con I-41).
- Fasi chiuse: **F0, F1, F2, F3**. F4a aperta (serve telefono Android; Arti non regge su Windows → ADR-0016).

## Fatto il 16/10
1. KEYS.md e report: K5/K13 nel vault, passphrase obbligatoria, compromesso scritto; debolezze 3–5 e 13 chiuse nel report.
2. **Musica e topic dentro v3.0**: musica = Blossom + HLS audio in F6 (I-16 estesa all'audio); topic = NIP-72 in nuova fase **F6b** (proposta I-42). Stima totale ~45–59 giornate.
3. **Q18 deciso**: procedura DSA notice and action scritta e approvata **prima di iniziare F6** (precondizione in MIGRATION_PLAN). Q19 (moderazione topic) aperta.
4. **Bozza** `docs/design/PEZZI_MANCANTI.md`: monitoraggio, registrazione creator (mappa minima e cifrata con K17, nessuno storico, proposta I-50), shop fisico (K16, indirizzi cancellati dopo 30 gg), boost (ordinamento nell'app, "Promosso", tetto posizioni). **Da approvare.**

## Fatto il 16/10, seconda parte (F4 iniziata)
5. GitHub Actions fissate per hash (checkout v7.0.1, setup-node v7.1.0), `persist-credentials: false`, cache spenta; I-25 lo controlla.
6. Bozza `docs/legal/PROCEDURA_DSA.md` (proposte I-51…I-56, K18). Punto chiave del fondatore: **Q20 SRLS** prima di hub reali, vendita H8, shop.
7. **I-37 riscritta (opzione A):** tor C non può rendere obbligatoria l'autenticazione SOCKS (provato); garanzie reali = `OnionTrafficOnly` (rifiuto con codice 2), circuiti isolati, solo 127.0.0.1, cookie, tor muore con l'app, hash fissato. ADR-0016 corretta.
8. **Crate `app/transport`** (I-36: percorso aggiornato): `OnionV3` validato (vettore indipendente Python), trait `Transport`, `TorProcessTransport`. Con tor vero: onion di prova raggiunto in ~37 s. `app/tor-windows.lock.json` con gli hash del Tor Expert Bundle 15.0.21 (firma GPG delle somme ancora da verificare, F8).
9. **Sabotaggio irrobustito:** timeout = "interrotta" (non più "presa"), `timeoutMs` per prova; test I-37 "solo onion" ora pretende il codice 2; prova "muore con l'app" senza esempio di cargo (non veniva ricompilato); test I-39 riprova irrobustito (16 bit + log). **48/48 prese** da campagna vera con tor.
10. Prove locali con tor: `M4TR1X_TOR_BIN=<tor.exe verificato>`; il tor di F4a è nello scratchpad della sessione del 15/10 (non nel repo).

## Prossimo
- OK del fondatore sulla bozza PEZZI_MANCANTI (tabella in fondo) → numeri I-42…I-50, K16, K17 entrano in SECURITY_INVARIANTS/KEYS.
- Scrivere la procedura DSA (precondizione di F6).
- **F4 continua:** guscio Tauri (I-31, CSP), identità dal seme, feed/post/profilo, I-01; `ArtiTransport` per Linux; `cargo deny` in CI.

## Fuori dal repo, ancora aperto
- Sito nderja.com: correggere "identità ML-DSA", "codice pubblico", download Kalendae rotto.
- Telefono Android per F4a.
- Repo v2: rotazione di `h8identity.enc` mai fatta.
