# ADR-0006 — Video: HLS via hub; niente WebRTC

**Contesto.** Oggi la diretta usa WebRTC, che cerca connessioni dirette, rivela gli IP (anche quello locale) e non passa da Tor: contraddice "Tor in tutte le app". Inoltre ogni nodo converte i video da sé, troppo pesante per i nodi piccoli.

**Decisione.** Conversione solo sugli hub (ffmpeg, HLS 480p/720p, segmenti come blob Blossom, manifest firmato dall'hub). Edge e nodi fanno solo cache. **Niente diretta in v3.0**; una "quasi-diretta" HLS con 20–40 secondi di ritardo si valuta dopo (OPEN_QUESTIONS Q6).

**Conseguenze.**
- (+) Una sola strada per i video; anonimato coerente anche per chi trasmette.
- (+) I nodi piccoli non hanno più bisogno di ffmpeg.
- (−) Niente diretta "vera" al lancio. Su Tor la banda per utente è limitata: 480p di default.
- (−) Gli hub contano molto (2, con backup reciproco). Se cadono entrambi, i video già convertiti restano su edge e nodi; i nuovi aspettano.
