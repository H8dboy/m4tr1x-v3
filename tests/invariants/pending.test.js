// @ts-check
'use strict'
/**
 * Invarianti non ancora attive: compaiono nell'output come TODO, con la fase in
 * cui diventano obbligatorie (docs/SECURITY_INVARIANTS.md). Quando una fase le
 * attiva, la riga si toglie da qui e nasce il file inv-xx-*.test.js.
 */
const { test } = require('node:test')

const PENDING = [
  ['I-01', 'nessuna richiesta delle app fuori da Tor', 'F4'],
  ['I-09', 'conio senza firma della radice rifiutato', 'F7'],
  ['I-10', 'spesa senza autorizzazione del wallet rifiutata', 'F7'],
  ['I-11', 'conservazione dei saldi', 'F7'],
  ['I-12', 'doppia spesa impossibile (seq per conto)', 'F7'],
  ['I-13', 'equivocazione dell\'emittente scoperta', 'F7'],
  ['I-14', 'conti H8 non collegabili alle chiavi Nostr', 'F7'],
  ['I-16', 'metadati tolti sul dispositivo', 'F6'],
  ['I-19', 'DM solo come gift wrap 1059', 'F5'],
  ['I-20', 'busta con chiave usa e getta e data sfalsata', 'F5'],
  ['I-21', 'aggiornamento con firma non valida rifiutato', 'F8'],
  ['I-22', 'niente rollback di versione o seq', 'F8'],
  ['I-24', 'build riproducibile dove possibile', 'F8'],
  ['I-27', 'processi esterni con timeout', 'F6'],
  ['I-31', 'nessuna chiave privata nel JavaScript dell\'app; CSP stretta', 'F4'],
  ['I-32', 'buoni offline entro il tetto', 'F10'],
  ['I-34', 'nessun trasferimento tra utenti; split della mancia', 'F7'],
  ['I-35', 'buono cieco non collegabile al bonifico', 'F7b'],
  ['I-36', 'solo il modulo transport apre connessioni; solo onion v3', 'F4'],
  ["I-37", "Tor di Windows usabile solo dall'app", "F4"],
]

for (const [id, what, phase] of PENDING) test(`${id} — ${what}`, { todo: `attiva in ${phase}` }, () => {})
