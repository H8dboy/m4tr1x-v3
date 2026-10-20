// Prova per tests/unit/report-esito.test.js: un test che fallisce.
'use strict'
const test = require('node:test')
const assert = require('node:assert')
test('uno', () => {})
test('due', () => { assert.equal(1, 2) })
