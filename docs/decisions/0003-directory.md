# ADR-0003 — Directory: descrittori firmati su relay Nostr delle 3 ancore

**Contesto.** Senza head centrale l'app deve trovare nodi, edge e hub. Valutati DHT Kademlia, relay Nostr come directory e gossip firmato (confronto in ARCHITECTURE §3).

**Decisione.** Ogni nodo pubblica un **descrittore firmato** (evento Nostr sostituibile: chiave del nodo, onion, ruoli, versione, scadenza ≤ 7 giorni, **nessun IP**) sulle 3 ancore. Le ancore si copiano a vicenda solo gli eventi con firma valida e applicano regole minime anti-Sybil. L'app scarica l'elenco completo da tutte e 3, unisce e verifica.

**Conseguenze.**
- (+) Nessun protocollo nuovo; il relay è già irrobustito.
- (+) L'app sceglie il nodo in locale: nessuno sa a quale si collegherà.
- (−) Se cadono tutte e 3 le ancore, i nodi nuovi non si trovano finché una torna (quelli in cache sì). Qualsiasi nodo può diventare ancora di riserva.
- (−) Un'ancora può nascondere nodi, non inventarli. Mitigazione: lettura da tutte e 3.
