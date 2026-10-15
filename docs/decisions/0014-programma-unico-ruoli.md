# ADR-0014 — Un solo programma server con ruoli

**Contesto.** Nodo utente, edge, hub e ancora fanno in gran parte le stesse cose (relay, blob, Tor, directory) in quantità diverse.

**Decisione.** Un solo programma `m4tr1x-node` con `role = user | edge | hub | anchor` in configurazione. Le funzioni specifiche (conversione video per l'hub, replica della directory per l'ancora, buoni offline per l'edge) sono moduli accesi solo nel loro ruolo, con un test che li verifica spenti negli altri (I-02). L'emittente H8 resta un programma separato (chiavi e rischi diversi).

**Conseguenze.**
- (+) Un solo codice, un solo aggiornamento; chiunque può fare da ancora di riserva.
- (−) Il pacchetto contiene moduli che su un ruolo restano spenti. ffmpeg non è nel pacchetto: si installa solo sugli hub.
