// Prova per tests/unit/report-esito.test.js: si blocca e non stampa niente.
'use strict'
const test = require('node:test')
setInterval(() => {}, 1000)
test('appeso', () => new Promise(() => {}))
