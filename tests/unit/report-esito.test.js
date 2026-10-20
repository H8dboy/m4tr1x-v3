// @ts-check
'use strict'
/**
 * Lo strumento che dice "verde" (npm run report) non deve mai dirlo quando un test
 * non ha girato: un file che non risponde conta come FALLITO, sempre.
 * Le prove usano processi veri (tests/fixtures/report/), non solo testo finto:
 * i casi sono quelli visti con Node 22.23.3 il 20/10/2026.
 */
const test = require('node:test')
const assert = require('node:assert/strict')
const path = require('path')
const { spawnSync } = require('child_process')
const { esito, eseguiFile } = require('../../tools/report/esito')

const ROOT = path.resolve(__dirname, '..', '..')
const F = (/** @type {string} */ n) => `tests/fixtures/report/${n}.js`
const esegui = (/** @type {string} */ n, timeoutMs = 60000) => eseguiFile(F(n), { cwd: ROOT, timeoutMs, testTimeoutMs: 600000 })

test('file con test che passano: verde, nessun problema', () => {
  const t = esegui('buono')
  assert.equal(t.problema, null)
  assert.equal(t.pass, 2)
  assert.equal(t.fail, 0)
})

test('le diagnostiche dei test arrivano anche da verde (fasi, tempi), senza il riepilogo del runner', () => {
  const t = esegui('buono')
  assert.deepEqual(t.diagnostiche, ['fase di prova=0.02s'])
})

test('file con un test che fallisce: 1 fallito', () => {
  const t = esegui('rotto')
  assert.equal(t.pass, 1)
  assert.equal(t.fail, 1)
})

test('file bloccato: fermato dal tempo massimo → fallito, con il motivo', () => {
  const t = esegui('appeso', 3000)
  assert.equal(t.pass, 0)
  assert.ok(t.fail >= 1, 'un file bloccato deve contare come fallito')
  assert.match(String(t.problema), /processo fermato/)
  assert.match(String(t.problema), /nessun riepilogo/)
})

test('file che esce con codice 0 a metà: fallito (il test mancante non sparisce)', () => {
  const t = esegui('muto')
  assert.ok(t.fail >= 1, 'uscita anticipata con codice 0 deve contare come fallito')
  assert.match(String(t.problema), /nessun riepilogo/)
})

test('file senza test: fallito (Node lo riporterebbe come 1 verde)', () => {
  const t = esegui('vuoto')
  assert.ok(t.fail >= 1)
  assert.match(String(t.problema), /nessun test/)
})

test('il caso del 20/10: nessuna uscita → mai 0 verdi 0 falliti', () => {
  for (const r of [
    { status: 0, signal: null, out: '' },
    { status: null, signal: null, out: '', error: new Error('spawnSync node ETIMEDOUT') },
    { status: null, signal: 'SIGKILL', out: 'TAP version 13\n' },
  ]) {
    const e = esito(r)
    assert.ok(e.fail >= 1, `deve fallire: ${JSON.stringify(r)}`)
    assert.ok(e.problema)
  }
})

test('runner della CI (esegui.js): esce con 1 su un file fallito, --senza-saltati fa fallire i saltati', () => {
  const cli = (/** @type {string[]} */ a) => {
    const env = { ...process.env }
    delete env.NODE_TEST_CONTEXT
    return spawnSync(process.execPath, ['tools/report/esegui.js', ...a], { cwd: ROOT, encoding: 'utf8', env, timeout: 120000 })
  }
  assert.equal(cli([F('buono')]).status, 0)
  assert.equal(cli([F('rotto')]).status, 1)
  assert.equal(cli([F('vuoto')]).status, 1, 'un file senza test non passa in CI')
  assert.equal(cli([F('buono'), F('muto')]).status, 1, 'un file uscito a metà non passa in CI')
  assert.equal(cli([F('saltato')]).status, 0, 'senza --senza-saltati un test saltato è ammesso')
  assert.equal(cli(['--senza-saltati', F('saltato')]).status, 1, 'con --senza-saltati un test saltato fa fallire')
})

test('riepilogo con fail 0 ma uscita diversa da 0, o test annullati: fallito', () => {
  assert.ok(esito({ status: 1, signal: null, out: '# tests 2\n# pass 2\n# fail 0\n' }).fail >= 1)
  const e = esito({ status: 1, signal: null, out: '# tests 1\n# pass 0\n# fail 0\n# cancelled 1\n' })
  assert.equal(e.fail, 1)
  assert.match(String(e.problema), /annullati/)
})
