// @ts-check
'use strict'
/**
 * Avvio del nodo da file (node/src/cli.js, node/bin/m4tr1x-node.js): in produzione la configurazione è
 * un file JSON con un elenco CHIUSO di chiavi. Opzioni di prova, passphrase in chiaro, chiave
 * d'amministrazione e permesso di cambiare versione di Node non si possono scrivere nel file.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const os = require('os')
const path = require('path')
const { spawn, spawnSync } = require('child_process')
const { leggiConfig, fileDagliArgomenti, CHIAVI_AMMESSE } = require('../../node/src/cli')

const ROOT = path.resolve(__dirname, '..', '..')
const BIN = path.join(ROOT, 'node', 'bin', 'm4tr1x-node.js')
const PASS = 'passphrase dei test, non segreta, abbastanza lunga'

/** Cartella di prova con dati, passphrase (fuori dai dati) e un file di configurazione. */
function prepara(/** @type {object} */ extra = {}, /** @type {{ pass?: string, passDentro?: boolean }} */ o = {}) {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'm4-cli-'))
  const dataDir = path.join(base, 'dati')
  fs.mkdirSync(dataDir)
  const keyPassphraseFile = path.join(o.passDentro ? dataDir : base, 'passphrase')
  fs.writeFileSync(keyPassphraseFile, o.pass ?? PASS, { mode: 0o600 })
  const file = path.join(base, 'node.json')
  fs.writeFileSync(file, JSON.stringify({ dataDir, keyPassphraseFile, network: 'm4tr1x-test', tor: false, port: 0, ...extra }))
  return { base, dataDir, file, keyPassphraseFile }
}

test('una configurazione minima passa, con le sole chiavi scritte nel file', () => {
  const p = prepara()
  const c = leggiConfig(p.file)
  assert.deepEqual(Object.keys(c).sort(), ['dataDir', 'keyPassphraseFile', 'network', 'port', 'tor'])
})

test('opzioni di prova, segreti in chiaro e chiavi sconosciute sono rifiutate per nome', () => {
  const vietate = {
    allowLoopbackPeersForTests: true, onBlobServitoForTests: 'x', descriptorOnionForTests: 'x', qualcosaForTest: 1,
    keyPassphrase: PASS, adminKey: 'a'.repeat(64), allowNodeVersionMismatch: true, log: 'x', onFatal: 'x', chiaveInventata: 1,
  }
  for (const [k, v] of Object.entries(vietate)) {
    assert.ok(!CHIAVI_AMMESSE.includes(k), `${k} è tra le chiavi ammesse`)
    assert.throws(() => leggiConfig(prepara({ [k]: v }).file), new RegExp(k), `${k} accettata dal file`)
  }
})

test('percorsi relativi, passphrase dentro i dati, corta o mancante: il nodo non parte', () => {
  assert.throws(() => leggiConfig(prepara({ dataDir: 'dati' }).file), /dataDir/)
  assert.throws(() => leggiConfig(prepara({}, { passDentro: true }).file), /FUORI dalla cartella dati/)
  assert.throws(() => leggiConfig(prepara({}, { pass: 'corta' }).file), /troppo corta/)
  const p = prepara()
  fs.rmSync(p.keyPassphraseFile)
  assert.throws(() => leggiConfig(p.file), /passphrase non trovato/)
  assert.throws(() => leggiConfig(prepara({ tor: true }).file), /torBin/)
  assert.throws(() => leggiConfig(prepara({ tor: true, torBin: 'tor' }).file), /torBin/)
  const rotto = prepara()
  fs.writeFileSync(rotto.file, '{ non json')
  assert.throws(() => leggiConfig(rotto.file), /JSON valido/)
  fs.writeFileSync(rotto.file, '[]')
  assert.throws(() => leggiConfig(rotto.file), /oggetto JSON/)
})

test('passphrase leggibile da altri utenti: rifiutata (dove i permessi esistono)', { skip: process.platform === 'win32' ? 'Windows non ha i permessi POSIX' : false }, () => {
  const p = prepara()
  fs.chmodSync(p.keyPassphraseFile, 0o644)
  assert.throws(() => leggiConfig(p.file), /leggibile da altri utenti/)
})

test('argomenti: solo --config <file>', () => {
  assert.throws(() => fileDagliArgomenti([]), /uso:/)
  assert.throws(() => fileDagliArgomenti(['--config']), /uso:/)
  assert.throws(() => fileDagliArgomenti(['x', '--config']), /uso:/)
  assert.throws(() => fileDagliArgomenti(['--config', 'a', 'b']), /uso:/)
  assert.equal(fileDagliArgomenti(['--config', 'a.json']), path.resolve('a.json'))
})

test('il programma esce con 1 e dice cosa non va, senza avviare niente', () => {
  const p = prepara({ allowLoopbackPeersForTests: true })
  const r = spawnSync(process.execPath, [BIN, '--config', p.file], { encoding: 'utf8', timeout: 30000 })
  assert.equal(r.status, 1)
  assert.match(r.stderr, /allowLoopbackPeersForTests/)
  assert.ok(!fs.existsSync(path.join(p.dataDir, 'secrets.vault.json')), 'il nodo ha creato il vault con una configurazione rifiutata')
})

test('il programma avvia il nodo dal file e crea il vault delle chiavi', { timeout: 120000 }, async () => {
  const p = prepara()
  const figlio = spawn(process.execPath, [BIN, '--config', p.file], { stdio: ['ignore', 'pipe', 'pipe'] })
  let uscita = ''
  figlio.stdout.on('data', d => { uscita += d })
  figlio.stderr.on('data', d => { uscita += d })
  try {
    const t0 = Date.now()
    while (!/nodo avviato: ruolo user, rete m4tr1x-test/.test(uscita)) {
      if (figlio.exitCode !== null) throw new Error(`il nodo è uscito (${figlio.exitCode}): ${uscita}`)
      if (Date.now() - t0 > 90000) throw new Error(`il nodo non è partito in 90 s: ${uscita}`)
      await new Promise(r => setTimeout(r, 200))
    }
    assert.match(uscita, /\(solo locale\)/, 'il nodo non ascolta solo in locale')
    assert.ok(fs.existsSync(path.join(p.dataDir, 'secrets.vault.json')))
    assert.ok(!uscita.includes(PASS), 'la passphrase è finita nel log')
  } finally {
    figlio.kill()
    await new Promise(r => figlio.once('exit', r))
  }
})
