// @ts-check
'use strict'
/**
 * Invarianti non ancora attive: compaiono nell'output come TODO, con la fase in
 * cui diventano obbligatorie (docs/SECURITY_INVARIANTS.md). Quando una fase le
 * attiva, la riga si toglie da qui e nasce il file inv-xx-*.test.js.
 */
const { test } = require('node:test')

const PENDING = [
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
  ['I-32', 'buoni offline entro il tetto', 'F10'],
  ['I-34', 'nessun trasferimento tra utenti; split della mancia', 'F7'],
  ['I-35', 'buono cieco non collegabile al bonifico', 'F7b'],
  ['I-42', 'approvazioni dei topic solo da moderatori correnti', 'F6b'],
  ['I-43', 'metriche del nodo senza IP, pubkey, onion, id', 'F2b'],
  ['I-44', 'raggiungibilita firmata da 2 ancore su 3', 'F3b'],
  ['I-45', 'registrazione creator con doppia firma', 'F7'],
  ['I-46', 'mance senza pubkey creator ne id conto in pubblico', 'F7'],
  ['I-47', 'indirizzo cifrato per K16 e cancellato dopo 30 giorni', 'F7c'],
  ['I-48', 'boost etichettato, max 10 posizioni, solo scopri', 'F7'],
  ['I-49', 'boost solo con riga firmata nel registro', 'F7'],
  ['I-50', 'mappa creator minima e cifrata (K17)', 'F7'],
  ['I-57', 'ritiro al distributore senza dati personali, codice monouso', 'F10'],
]

for (const [id, what, phase] of PENDING) test(`${id} — ${what}`, { todo: `attiva in ${phase}` }, () => {})
