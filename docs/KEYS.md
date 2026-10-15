# M4TR1X v3 — Chiavi

> Principio 4: ogni ruolo ha la sua chiave. Una chiave rubata deve far danni **solo nel suo ruolo**.
> Ogni firma include un'etichetta di dominio (`m4tr1x/<ruolo>/v1`) e l'identificativo di rete: una firma non è mai riutilizzabile in un altro contesto (I-08).
> Nessuna chiave privata, chiave onion o keystore entra mai in un repo o nella CI (I-28), salvo dove indicato esplicitamente ("segreto CI").

## Riepilogo

| ID | Chiave | Algoritmo | Dove vive | Online? | Danno se rubata |
|---|---|---|---|---|---|
| K0 | Seme utente (24 parole) | BIP-39 → HKDF | Carta, dall'utente | No | Tutto l'utente |
| K1 | Identità | secp256k1 Schnorr (Nostr) | Portachiavi del dispositivo | Sì | Impersonare, leggere i DM |
| K1d | Chiave DM | secp256k1 (NIP-44) | Portachiavi del dispositivo | Sì | Leggere i DM del suo periodo |
| K1r | Revoca | secp256k1 | **Solo dal seme**, mai salvata | No | Revocare l'identità (solo con il seme) |
| K2 | Wallet | secp256k1 Schnorr | Portachiavi del dispositivo | Sì | Spendere il saldo |
| K5 | Nodo | secp256k1 Schnorr | Disco del nodo, cifrata | Sì | Impersonare il nodo nella directory |
| K6 | Servizio onion | Ed25519 (Tor v3) | Cartella di Tor del nodo | Sì | Impersonare l'indirizzo onion |
| K7 | Ancora | secp256k1 Schnorr | Disco dell'ancora | Sì | Firmare istantanee della directory a nome dell'ancora |
| K8 | Hub | secp256k1 Schnorr | Disco dell'hub | Sì | Firmare manifest video falsi |
| K9 | **Radice emittente H8** | ML-DSA-65 | **Offline**, backup diviso | No | Coniare H8, certificare emittenti |
| K10 | Emittente operativo H8 | ML-DSA-65 | Server dell'emittente | Sì | Ordinare/censurare le spese (non rubare, non coniare) |
| K11 | **Firma release** | Ed25519 (minisign) | **Offline**, dal fondatore | No | Aggiornamenti malevoli (insieme a K12 su Android) |
| K12 | Keystore Android | RSA/EC (Android) | Offline + segreto CI cifrato (OPEN_QUESTIONS Q9) | Solo in build | APK con la stessa firma |
| K13 | Admin del nodo | Token casuale 256 bit | File del nodo (permessi 600) | Sì | Amministrare quel nodo |
| K14 | ~~Pagamenti~~ | — | **Non serve** (bonifico manuale, ADR-0015) | — | — |
| K15 | Firma cieca dei buoni H8 | RSA-PSS 3072 (RFC 9474) | Server dell'emittente | Sì | Emettere buoni falsi (limitato: i coni restano con K9 e la conservazione è verificata) |

## Dettaglio

### K0 — Seme utente
- **Scopo:** unico backup dell'utente; da qui si ricavano K1, K1d, K1r, K2 con etichette diverse.
- **Dove vive:** su carta. Nell'app esiste solo durante la creazione e il recupero, poi viene cancellato dalla memoria.
- **Rotazione:** non si ruota; se è compromesso si crea una nuova identità e si usa K1r per revocare la vecchia.
- **Se rubato:** il ladro è l'utente. Mitigazione: il seme non sta mai in digitale; l'app lo mostra una volta e chiede di riscriverne 3 parole a caso per conferma.

### K1 — Identità (Nostr)
- **Scopo:** firma post, profilo, follow, eventi di migrazione. È "il nome" dell'utente.
- **Dove vive:** Android Keystore / iOS Keychain / DPAPI / macOS Keychain; altrimenti passphrase + scrypt (vault `packages/keys`, AES-256-GCM). **La firma avviene nel lato Rust dell'app**, il JavaScript non vede mai la chiave (I-31).
- **Rotazione:** evento di migrazione firmato con K1 vecchia → K1 nuova (ricavata con indice di versione successivo dal seme).
- **Se rubata:** il ladro può pubblicare a nome dell'utente e leggere i DM se K1d non è separata. Rimedio: revoca con K1r e migrazione.

### K1d — Chiave DM
- **Scopo:** ricevere e decifrare i DM; annunciata da K1 in un evento firmato. Separata per poterla ruotare spesso (es. ogni 30 giorni) senza cambiare identità.
- **Se rubata:** espone i DM del periodo in cui era attiva.

### K1r — Revoca
- **Scopo:** dichiarare morta K1 anche se il ladro la possiede.
- **Dove vive:** non viene mai salvata; si ricava dal seme quando serve.

### K2 — Wallet H8
- **Scopo:** autorizzare le spese H8 (`m4tr1x/h8/spend/v1`). È distinta da K1: il registro non deve collegare conto e identità (I-14).
- **Rotazione:** riga `ricodifica_conto` firmata dalla K2 vecchia, o da K0 se K2 è persa.
- **Se rubata:** può spendere fino al saldo. Mitigazione: conferma con PIN/biometria sopra una soglia.

### K5 / K6 — Nodo e onion
- **Scopo:** K5 firma il descrittore nella directory; K6 è la chiave di Tor dell'indirizzo onion.
- **Dove vivono:** sul nodo, fuori dal repo, permessi 600; K5 cifrata con chiave della macchina.
- **Rotazione:** nuovo descrittore con nuova chiave + evento di revoca firmato con la vecchia; nuovo onion.
- **Se rubate:** impersonazione del nodo. I client verificano sempre gli hash dei blob (I-17): un nodo falso può non servire, non può servire contenuti alterati.

### K7 — Ancora
- **Scopo:** firmare le istantanee della directory (per il confronto tra ancore).
- **Se rubata:** può pubblicare istantanee false *a suo nome*; le altre due ancore e i client si accorgono della discrepanza. Rotazione: le altre due ancore co-firmano l'annuncio della chiave nuova.

### K8 — Hub
- **Scopo:** firmare i manifest HLS ("questi segmenti sono la conversione di questo video, firmato dall'autore X").
- **Se rubata:** manifest falsi a nome dell'hub. Mitigazione: il manifest riporta anche l'hash dell'evento video firmato dall'autore; revoca della chiave hub nella directory.

### K9 — Radice emittente H8
- **Scopo:** coniare H8 e certificare K10 (con scadenza ≤ 90 giorni).
- **Dove vive:** computer offline (mai collegato a internet) o chiavetta dedicata; backup diviso 2-su-3 (Shamir) tra persone di fiducia (OPEN_QUESTIONS Q9).
- **Uso:** raro, a lotti (es. un conio al mese firmato offline e portato online su chiavetta).
- **Se rubata:** coniare H8 dal nulla. Mitigazione: i coni sono pubblici nel registro, quindi uno strano si vede; rotazione = nuova radice annunciata in una release firmata K11 (le app conoscono la radice valida dalla release).

### K10 — Emittente operativo H8
- **Scopo:** firmare le righe in ordine. **Non può coniare** (serve K9) e **non può spendere al posto degli utenti** (serve K2).
- **Rotazione:** automatica alla scadenza del certificato; immediata in caso di furto (nuovo certificato K9, il vecchio viene messo in lista di revoca).
- **Se rubata:** il ladro può scrivere righe ordinate in modo diverso (equivocazione, scoperta da I-13) o rifiutare spese. Non può rubare.

### K11 — Firma release
- **Scopo:** firmare `release.json` per app e nodi.
- **Dove vive:** offline, dal fondatore; copia di backup in un secondo luogo fisico. **Mai in CI.**
- **Rotazione:** la release N firmata con la chiave vecchia annuncia la chiave nuova; dalla N+1 si usa la nuova.
- **Se rubata:** aggiornamenti malevoli per desktop e nodi; per Android serve anche K12. È la chiave più pericolosa insieme a K9.

### K12 — Keystore Android
- **Scopo:** firma dell'APK. Android accetta aggiornamenti solo con la stessa firma: perderla = gli utenti devono reinstallare.
- **Dove vive:** originale offline + copia cifrata come segreto della CI (per costruire gli APK) oppure firma locale dopo la build (OPEN_QUESTIONS Q9).
- **Se rubata:** APK falsi con firma valida, ma l'updater dell'app li rifiuta senza K11 (I-21).

### K13 — Admin del nodo
- **Scopo:** API di amministrazione del singolo nodo, solo da loopback (I-33). Non viene mai stampata nei log (correzione B1 del 15/10).

### K14 — Fornitore di pagamenti
- **Non serve:** l'acquisto avviene con bonifico SEPA manuale (ADR-0015). Le coordinate bancarie di H8-Group non sono una chiave e non stanno nel codice.

### K15 — Firma cieca dei buoni
- **Scopo:** firmare i buoni senza vederli, così il riscatto non è collegabile al bonifico (I-35).
- **Dove vive:** server dell'emittente. Ruotata a ogni lotto di vendita; i buoni firmati con una chiave scaduta non si riscattano più dopo un periodo di grazia.
- **Se rubata:** si possono creare buoni falsi. Danno limitato: ogni riscatto consuma H8 da un lotto coniato con K9, quindi i buoni falsi esauriscono il lotto e la differenza con i bonifici incassati si vede alla riconciliazione.

## Cerimonia di avvio (una volta, prima del lancio)

1. Computer offline pulito. Generare K9 e K11.
2. Backup di K9 diviso 2-su-3, di K11 in due luoghi.
3. Pubblicare le chiavi pubbliche K9 e K11: nel codice dell'app, nel README, su nderja.com.
4. Generare K12 (keystore Android) e conservarla come K11.
5. Annotare tutto in un registro cartaceo (data, chi era presente, impronte delle chiavi).
