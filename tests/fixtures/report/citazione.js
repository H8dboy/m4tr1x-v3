// Prova per tests/unit/report-esito.test.js: errore di CODICE che cita la frase di una fase di rete: mai rilanciato.
'use strict'
const test = require('node:test')
const fs = require('fs')
const f = process.env.M4TR1X_PROVA_CONTATORE
if (f) fs.writeFileSync(f, String((fs.existsSync(f) ? Number(fs.readFileSync(f, 'utf8')) : 0) + 1))
test('controllo statico', () => { throw new Error('nel sorgente manca la fase "pubblicazione"') })
