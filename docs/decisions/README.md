# Decisioni (ADR)

Un file per ogni scelta importante: **contesto → decisione → conseguenze**.
Stato possibile: *proposta* (in attesa dell'OK del fondatore), *accettata*, *sostituita da ADR-xxxx*.
Un ADR non si cancella: se si cambia idea se ne scrive uno nuovo che lo sostituisce.

| ADR | Titolo | Stato |
|---|---|---|
| [0001](0001-monorepo.md) | Monorepo (emittente in repo privato separato) | accettata 15/10 |
| [0002](0002-guscio-tauri-arti.md) | Un solo guscio per le app: Tauri 2 + Arti | accettata **con condizione**: prototipo F4a |
| [0003](0003-directory.md) | Directory: descrittori firmati su relay Nostr delle 3 ancore | proposta |
| [0004](0004-nostr-nip17.md) | Post e DM: Nostr + NIP-17 | proposta |
| [0005](0005-blossom.md) | Media: Blossom, upload a pezzi | proposta |
| [0006](0006-video-hls-no-webrtc.md) | Video: HLS via hub; niente WebRTC | proposta |
| [0007](0007-h8-registro-emittente.md) | H8: registro firmato dall'emittente, radice offline | accettata 15/10 (mance solo ai creator) |
| [0008](0008-chiavi-separate.md) | Chiavi separate ricavate da un seme | proposta |
| [0009](0009-aggiornamenti-firmati.md) | Aggiornamenti firmati con manifest e anti-rollback | proposta |
| [0010](0010-via-federazioni-p2p.md) | Via le federazioni con Mastodon/PeerTube/Funkwhale e il P2P | proposta |
| [0011](0011-sqlite.md) | SQLite ovunque; via Postgres e l'head | proposta |
| [0012](0012-javascript-ts-check.md) | Server in JavaScript con `@ts-check` | proposta |
| [0013](0013-tor-server-arti-app.md) | Tor: `tor` sui server, Arti nelle app | proposta |
| [0014](0014-programma-unico-ruoli.md) | Un solo programma server con ruoli | proposta |
| [0017](0017-due-famiglie-di-firme.md) | Due famiglie di firme: Schnorr per il social, ML-DSA-65 per ciò che vale | accettata 15/10 |
| [0018](0018-annuncio-chiave-dm.md) | Messaggi privati: chiave DM separata, annunciata dall'identità (kind 10477 con prova di possesso) | proposta 25/10 |
| [0016](0016-interfaccia-trasporto.md) | Interfaccia di trasporto unica: Arti dove regge, Tor classico su Windows | accettata 15/10 |
| [0015](0015-h8-acquisto-sepa.md) | H8: acquisto con bonifico SEPA manuale, buoni | accettata 15/10 (in attesa di parere legale) |
