// Prova per tests/unit/report-esito.test.js: due test che passano.
'use strict'
const test = require('node:test')
test('uno', () => {})
test('due', async () => { await new Promise(r => setTimeout(r, 20)) })
