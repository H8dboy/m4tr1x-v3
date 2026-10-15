# ADR-0007 — H8: registro firmato dall'emittente, radice offline

**Contesto.** Oggi ogni nodo ha un proprio registro con la propria genesi: nessuna protezione contro la doppia spesa tra nodi (C4 dell'audit). Decisione presa: niente blockchain né consenso distribuito.

**Decisione.** Un emittente unico mette in ordine e firma un registro append-only (catena di hash). Il conio richiede la **radice offline K9**; le righe le firma l'**emittente operativo K10** (certificato da K9, con scadenza); ogni spesa richiede l'**autorizzazione del wallet K2** del pagante, con numero di sequenza per conto. Ogni nodo, edge e ancora tiene una copia e la verifica da zero. I conti sono identificativi casuali, non collegati alle chiavi Nostr. Buoni offline per i distributori entro un tetto per conto e per giorno.

**Aggiornamento 15/10/2026 (ADR-0017):** anche il wallet K2 firma con ML-DSA-65: ogni riga del registro è resistente ai computer quantistici sia dal lato utente sia dal lato emittente.

**Conseguenze.**
- (+) Doppia spesa impossibile per costruzione (I-12); conio di nascosto impossibile (I-09, I-11); l'emittente non può rubare (I-10).
- (+) Molto più semplice di una blockchain; verificabile da chiunque.
- (−) **Contrasto dichiarato con "nessun single point of failure":** con l'emittente giù non si spende (salvo buoni offline). I social continuano a funzionare.
- (−) L'emittente può censurare una spesa e conosce la mappa chiave Nostr → conto dei creator che ricevono mance.
- **Aggiornamento 15/10/2026 (Q1):** mance solo verso conti creator, divise nello split; nessun trasferimento utente → utente. Il tipo di riga "trasferimento" non esiste (I-34). Destinazione della quota nodo: Q16.
- **Aggiornamento 15/10/2026 (Q3):** il codice dell'emittente è chiuso (repo privato); la sicurezza non dipende da esso perché ogni replica verifica il registro con codice pubblico.
