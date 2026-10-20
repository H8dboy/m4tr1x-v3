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
const fs = require('fs')
const os = require('os')
const { esito, eseguiFile, eseguiConRilancio } = require('../../tools/report/esito')

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

// ─── Rilancio unico per la rete Tor (fondatore, 20/10/2026) ──────────────────
/** esegue `nome` con il rilancio, contando quante volte gira davvero */
function conRilancio(/** @type {string} */ nome, rossi = 1, timeoutMs = 60000, fase = 'rete') {
  const contatore = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'm4-rilancio-')), 'n')
  const env = { ...process.env, M4TR1X_PROVA_CONTATORE: contatore, M4TR1X_PROVA_ROSSI: String(rossi), M4TR1X_PROVA_FASE: fase }
  const t = eseguiConRilancio(F(nome), { cwd: ROOT, timeoutMs, testTimeoutMs: 600000, env })
  const giri = fs.existsSync(contatore) ? Number(fs.readFileSync(contatore, 'utf8')) : 0
  return { t, giri }
}

test('rosso SOLO in fase "rete": un rilancio, e resta scritto', () => {
  const { t, giri } = conRilancio('rete', 1)
  assert.equal(giri, 2)
  assert.equal(t.fail, 0)
  assert.equal(t.rilancio?.fase, 'rete')
  assert.match(String(t.rilancio?.primoErrore), /fase "rete" fallita/)
})

test('fase "tor" (bootstrap fermo) e "tor-a"/"tor-b": un rilancio come la rete (fondatore, 20/10/2026)', () => {
  for (const fase of ['tor', 'tor-a', 'tor-b']) {
    const { t, giri } = conRilancio('rete', 1, 60000, fase)
    assert.equal(giri, 2, `fase "${fase}": non rilanciata`)
    assert.equal(t.rilancio?.fase, 'tor')
  }
})

test('una fase che INIZIA con "tor" ma non lo è: nessun rilancio', () => {
  const { t, giri } = conRilancio('rete', 1, 60000, 'torre')
  assert.equal(giri, 1)
  assert.equal(t.rilancio, undefined)
})

test('rosso di rete anche al secondo giro: resta rosso, mai un terzo giro', () => {
  const { t, giri } = conRilancio('rete', 5)
  assert.equal(giri, 2, 'un solo rilancio')
  assert.ok(t.fail >= 1)
})

test('rosso del nostro codice (fase "file"): nessun rilancio', () => {
  const { t, giri } = conRilancio('codice')
  assert.equal(giri, 1)
  assert.ok(t.fail >= 1)
  assert.equal(t.rilancio, undefined)
})

test('un rosso di rete e uno di codice nello stesso file: nessun rilancio', () => {
  const { t, giri } = conRilancio('misto')
  assert.equal(giri, 1)
  assert.equal(t.rilancio, undefined)
})

test('errore che CITA una fase di rete senza esserlo: nessun rilancio (20/10/2026)', () => {
  const { t, giri } = conRilancio('citazione')
  assert.equal(giri, 1, 'rilanciato un rosso di codice perché il messaggio citava "fase \\"pubblicazione\\""')
  assert.equal(t.rilancio, undefined)
})

test('file bloccato: mai rilanciato (non è la rete)', () => {
  const t = eseguiConRilancio(F('appeso'), { cwd: ROOT, timeoutMs: 3000, testTimeoutMs: 600000 })
  assert.ok(t.fail >= 1)
  assert.equal(t.rilancio, undefined)
  assert.ok(t.ms < 10000, `rilanciato? ${t.ms} ms`)
})

test('runner della CI: il rilancio si scrive nel log', () => {
  const contatore = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'm4-rilancio-')), 'n')
  const env = { ...process.env, M4TR1X_PROVA_CONTATORE: contatore, M4TR1X_PROVA_ROSSI: '1' }
  delete env.NODE_TEST_CONTEXT
  const r = spawnSync(process.execPath, ['tools/report/esegui.js', F('rete')], { cwd: ROOT, encoding: 'utf8', env, timeout: 120000 })
  assert.equal(r.status, 0, r.stdout + r.stderr)
  assert.match(r.stdout, /RILANCIO tests\/fixtures\/report\/rete\.js \(rete Tor, fase "rete"\)/)
})
