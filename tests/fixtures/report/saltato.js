// Prova per tests/unit/report-esito.test.js: un test verde e uno saltato.
'use strict'
const test = require('node:test')
test('uno', () => {})
test('due', { skip: 'prova saltata' }, () => {})
