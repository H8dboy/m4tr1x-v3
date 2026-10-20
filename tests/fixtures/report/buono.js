// Prova per tests/unit/report-esito.test.js: due test che passano.
'use strict'
const test = require('node:test')
test('uno', () => {})
test('due', async (t) => { t.diagnostic('fase di prova=0.02s'); await new Promise(r => setTimeout(r, 20)) })
