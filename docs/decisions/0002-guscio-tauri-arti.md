# ADR-0002 — Un solo guscio per le app: Tauri 2 + Arti

**Contesto.** Oggi convivono tre gusci: Electron (desktop), Capacitor (Android/iOS), Tauri (configurato, mai pubblicato). Ognuno gestisce Tor, permessi e CSP a modo suo. Electron porta un Chromium intero (oltre 100 MB e aggiornamenti continui: il 15/10 si è passati dalla 32 alla 44). Su Capacitor le POST non passano dal client che instrada su Tor e vanno gestite a mano.

**Decisione.** Tauri 2 per desktop, Android e iOS, con **Arti** (Tor in Rust) incorporato nel lato Rust. Tutta la rete dell'app passa da un solo modulo Rust che usa solo Arti. Le chiavi si usano solo nel lato Rust.

**Conseguenze.**
- (+) Un codice d'interfaccia, un modulo di rete, una politica di sicurezza per tutte le piattaforme.
- (+) App più piccola (usa la webview del sistema). Kalendae è già Tauri 2: l'esperienza c'è.
- (+) Le chiavi stanno fuori dal JavaScript (I-31).
- (−) Va scritto un po' di Rust (rete, firma, aggiornamenti).
- (−) **Da verificare con un prototipo (fase F4a)** che Arti come client verso servizi onion regga su Android e iOS. Piano B: Tor come libreria nativa (tor-android / Tor.framework) dietro la stessa interfaccia.
- **Aggiornamento 15/10/2026:** Arti 0.47 non regge su Windows (F4a). Vedi ADR-0016: interfaccia di trasporto unica, Tor classico solo su Windows.
- (−) Le webview differiscono tra piattaforme (Safari su iOS, WebView2 su Windows): più test d'interfaccia.
