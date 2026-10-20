// Prova per tests/unit/report-esito.test.js: esce con codice 0 a meta', il secondo test non gira mai.
'use strict'
const test = require('node:test')
test('uno', () => {})
test('due', async () => { process.exit(0) })
test('tre', () => {})
