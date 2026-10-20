// Prova per tests/unit/report-esito.test.js: rosso in fase "rete" per le prime M4TR1X_PROVA_ROSSI esecuzioni, poi verde.
'use strict'
const test = require('node:test')
const fs = require('fs')
/** conta le esecuzioni in M4TR1X_PROVA_CONTATORE; restituisce il numero di questa */
function esecuzione() {
  const f = process.env.M4TR1X_PROVA_CONTATORE
  const n = (f && fs.existsSync(f) ? Number(fs.readFileSync(f, 'utf8')) : 0) + 1
  if (f) fs.writeFileSync(f, String(n))
  return n
}
const n = esecuzione()
test('raggiunge l\'onion', () => { if (n <= Number(process.env.M4TR1X_PROVA_ROSSI || 1)) throw new Error('fase "rete" fallita dopo 300 s (massimo 300 s): scaduta') })
