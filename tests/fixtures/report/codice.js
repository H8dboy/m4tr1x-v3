// Prova per tests/unit/report-esito.test.js: rosso in fase "file" (il nostro codice): mai rilanciato.
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
esecuzione()
test('replica il file', () => { throw new Error('fase "file" fallita dopo 60 s (massimo 60 s): scaduta') })
