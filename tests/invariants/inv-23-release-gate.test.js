// @ts-check
'use strict'
/**
 * I-23 — La CI non pubblica release senza manifest firmato valido.
 * Firma: ML-DSA-65 con la chiave di release K11 (ADR-0017), dominio
 * "release/manifest". Si genera una chiave di prova, si firma un release.json e
 * si verifica che il cancello accetti solo il caso corretto.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const crypto = require('crypto')
const fs = require('fs')
const os = require('os')
const path = require('path')
const keys = require('../../packages/keys')
const protocol = require('../../packages/protocol')
const { verifyReleaseDir, signatureFile } = require('../../tools/release/verify-release')

const NET = 'm4tr1x-test'

/** @param {any} k @param {Buffer} manifest */
function sign(k, manifest, network = NET) {
  return signatureFile(protocol.signDomain(k, 'release/manifest', network, manifest), network)
}

/** @param {any} k @param {(dir: string) => void} [tweak] */
function goodRelease(k, tweak) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'm4-release-'))
  const pkg = Buffer.from('pacchetto finto')
  fs.writeFileSync(path.join(dir, 'm4tr1x-test.bin'), pkg)
  const manifest = Buffer.from(JSON.stringify({ version: '3.0.0-test', seq: 1, files: { 'm4tr1x-test.bin': crypto.createHash('sha256').update(pkg).digest('hex') } }))
  fs.writeFileSync(path.join(dir, 'release.json'), manifest)
  fs.writeFileSync(path.join(dir, 'release.json.sig'), sign(k, manifest))
  if (tweak) tweak(dir)
  return dir
}

const k = keys.generateKey('release')

test('release firmata correttamente (ML-DSA-65) → accettata', () => {
  assert.equal(verifyReleaseDir(goodRelease(k), k.publicKey, NET).version, '3.0.0-test')
})

test('firma assente → rifiutata', () => {
  const dir = goodRelease(k, d => fs.unlinkSync(path.join(d, 'release.json.sig')))
  assert.throws(() => verifyReleaseDir(dir, k.publicKey, NET), /non firmata/)
})

test('manifest modificato dopo la firma → rifiutato', () => {
  const dir = goodRelease(k, d => {
    const p = path.join(d, 'release.json')
    fs.writeFileSync(p, fs.readFileSync(p, 'utf8').replace('"seq":1', '"seq":2'))
  })
  assert.throws(() => verifyReleaseDir(dir, k.publicKey, NET), /firma non valida/)
})

test('firmato con un\'altra chiave di release → rifiutato', () => {
  assert.throws(() => verifyReleaseDir(goodRelease(k), keys.generateKey('release').publicKey, NET), /firma non valida/)
})

test('firmato per la rete di prova, verificato per la principale → rifiutato', () => {
  assert.throws(() => verifyReleaseDir(goodRelease(k), k.publicKey, 'm4tr1x-main'), /rete|firma non valida/)
})

test('firma Schnorr (famiglia sbagliata) al posto di ML-DSA → rifiutata', () => {
  const node = keys.generateKey('node')
  const dir = goodRelease(k, d => {
    const manifest = fs.readFileSync(path.join(d, 'release.json'))
    const digest = protocol.domainDigest('release/manifest', NET, manifest)
    const s = require('@noble/curves/secp256k1.js').schnorr.sign(digest, keys.exportSecret(node))
    fs.writeFileSync(path.join(d, 'release.json.sig'), signatureFile(Buffer.from(s).toString('hex'), NET))
  })
  assert.throws(() => verifyReleaseDir(dir, node.publicKey, NET), /chiave di release|firma non valida/)
})

test('pacchetto alterato → rifiutato', () => {
  const dir = goodRelease(k, d => fs.writeFileSync(path.join(d, 'm4tr1x-test.bin'), 'pacchetto cambiato'))
  assert.throws(() => verifyReleaseDir(dir, k.publicKey, NET), /hash diverso/)
})

test('file in più non elencato → rifiutato', () => {
  const dir = goodRelease(k, d => fs.writeFileSync(path.join(d, 'extra.bin'), 'x'))
  assert.throws(() => verifyReleaseDir(dir, k.publicKey, NET), /non elencato/)
})

test('la chiave identità o wallet non possono firmare una release', () => {
  const seed = keys.seedFromMnemonic(keys.generateMnemonic())
  for (const r of ['identity', 'wallet']) {
    assert.throws(() => protocol.signDomain(keys.deriveRoleKey(seed, r), 'release/manifest', NET, Buffer.from('x')), /ruolo/)
  }
})

test('senza chiave di release nel repo (prima della cerimonia) → nessuna release pubblicabile', () => {
  assert.throws(() => verifyReleaseDir(goodRelease(k)), /cerimonia/)
})
