# ADR-0010 — Via le federazioni con Mastodon/PeerTube/Funkwhale e il P2P

**Contesto.** Il nodo contiene adattatori per Mastodon, PeerTube e Funkwhale (adattatori locali, non ActivityPub completo), un tracker WebTorrent e un modulo P2P che il frontend non usa. Ognuno contatta server terzi o espone gli IP dei peer; senza Tor i media remoti rivelavano l'IP dell'utente (M10).

**Decisione.** Si tolgono tutti. Un solo protocollo per i contenuti: Nostr + Blossom.

**Conseguenze.**
- (+) Meno codice, meno superficie, nessuna richiesta verso server terzi.
- (−) Niente contenuti importati da altri social al lancio. Un ponte in sola lettura sugli hub si può valutare in futuro, come servizio separato e con un ADR suo.
