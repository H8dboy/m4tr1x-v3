// @ts-check
'use strict'
/**
 * I-23 — La CI non pubblica release senza manifest firmato valido.
 * Si genera una coppia di chiavi di prova, si firma un release.json in formato
 * minisign e si verifica che il cancello accetti solo il caso corretto.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const crypto = require('crypto')
const fs = require('fs')
const os = require('os')
const path = require('path')
const { verifyReleaseDir } = require('../../tools/release/verify-release')

const SPKI_PREFIX_LEN = 12

function testKey() {
  const { publicKey, privateKey } = crypto.generateKeyPairSync('ed25519')
  const raw = publicKey.export({ format: 'der', type: 'spki' }).subarray(SPKI_PREFIX_LEN)
  const keyId = crypto.randomBytes(8)
  const pubText = 'untrusted comment: chiave di prova\n' + Buffer.concat([Buffer.from('Ed'), keyId, raw]).toString('base64') + '\n'
  /** @param {Buffer} msg @param {string} trusted */
  const sign = (msg, trusted = 'timestamp:0') => {
    const sig = crypto.sign(null, crypto.createHash('blake2b512').update(msg).digest(), privateKey)
    const global = crypto.sign(null, Buffer.concat([sig, Buffer.from(trusted)]), privateKey)
    return ['untrusted comment: firma di prova', Buffer.concat([Buffer.from('ED'), keyId, sig]).toString('base64'),
      'trusted comment: ' + trusted, global.toString('base64'), ''].join('\n')
  }
  return { pubText, sign }
}

/** @param {(dir: string) => void} fill */
function releaseDir(fill) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'm4-release-'))
  fill(dir)
  return dir
}

function goodRelease(/** @type {ReturnType<typeof testKey>} */ k) {
  return releaseDir(dir => {
    const pkg = Buffer.from('pacchetto finto')
    fs.writeFileSync(path.join(dir, 'm4tr1x-test.bin'), pkg)
    const manifest = Buffer.from(JSON.stringify({ version: '3.0.0-test', seq: 1, files: { 'm4tr1x-test.bin': crypto.createHash('sha256').update(pkg).digest('hex') } }))
    fs.writeFileSync(path.join(dir, 'release.json'), manifest)
    fs.writeFileSync(path.join(dir, 'release.json.minisig'), k.sign(manifest))
  })
}

test('release firmata correttamente → accettata', () => {
  const k = testKey()
  assert.equal(verifyReleaseDir(goodRelease(k), k.pubText).version, '3.0.0-test')
})

test('firma assente → rifiutata', () => {
  const k = testKey(); const dir = goodRelease(k)
  fs.unlinkSync(path.join(dir, 'release.json.minisig'))
  assert.throws(() => verifyReleaseDir(dir, k.pubText), /non firmata/)
})

test('manifest modificato dopo la firma → rifiutato', () => {
  const k = testKey(); const dir = goodRelease(k)
  const p = path.join(dir, 'release.json')
  fs.writeFileSync(p, fs.readFileSync(p, 'utf8').replace('"seq":1', '"seq":2'))
  assert.throws(() => verifyReleaseDir(dir, k.pubText), /firma non valida/)
})

test('firmato con un\'altra chiave → rifiutato', () => {
  const k = testKey(); const other = testKey()
  assert.throws(() => verifyReleaseDir(goodRelease(k), other.pubText), /altra chiave/)
})

test('pacchetto alterato → rifiutato', () => {
  const k = testKey(); const dir = goodRelease(k)
  fs.writeFileSync(path.join(dir, 'm4tr1x-test.bin'), 'pacchetto cambiato')
  assert.throws(() => verifyReleaseDir(dir, k.pubText), /hash diverso/)
})

test('file in più non elencato → rifiutato', () => {
  const k = testKey(); const dir = goodRelease(k)
  fs.writeFileSync(path.join(dir, 'extra.bin'), 'x')
  assert.throws(() => verifyReleaseDir(dir, k.pubText), /non elencato/)
})

test('trusted comment manomesso → rifiutato', () => {
  const k = testKey(); const dir = goodRelease(k)
  const p = path.join(dir, 'release.json.minisig')
  fs.writeFileSync(p, fs.readFileSync(p, 'utf8').replace('timestamp:0', 'timestamp:1'))
  assert.throws(() => verifyReleaseDir(dir, k.pubText), /trusted comment/)
})

test('senza chiave di release nel repo (prima della cerimonia) → nessuna release pubblicabile', () => {
  const k = testKey()
  assert.throws(() => verifyReleaseDir(goodRelease(k)), /cerimonia/)
})
