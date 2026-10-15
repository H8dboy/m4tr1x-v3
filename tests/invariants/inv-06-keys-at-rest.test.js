// @ts-check
'use strict'
/**
 * I-06 — Nessuna chiave privata finisce su disco in chiaro.
 * Si crea un'identità completa, la si salva con il vault a passphrase e si
 * cerca su disco ogni rappresentazione delle chiavi e del seme: zero risultati.
 * Si verifica anche che un oggetto chiave serializzato non contenga il segreto.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const os = require('os')
const path = require('path')
const keys = require('../../packages/keys')

/** Tutte le forme in cui un segreto potrebbe comparire in un file. */
function encodings(/** @type {Uint8Array} */ bytes) {
  const b = Buffer.from(bytes)
  return [b.toString('hex'), b.toString('hex').toUpperCase(), b.toString('base64'), b.toString('base64url')]
}

function scanDir(/** @type {string} */ dir, /** @type {string[]} */ needles) {
  const hits = []
  for (const f of fs.readdirSync(dir)) {
    const raw = fs.readFileSync(path.join(dir, f))
    const text = raw.toString('latin1')
    for (const n of needles) if (text.includes(n)) hits.push(`${f}: contiene un segreto`)
    for (const n of needles.filter(x => /^[0-9a-f]+$/.test(x))) if (raw.includes(Buffer.from(n, 'hex'))) hits.push(`${f}: byte grezzi di un segreto`)
  }
  return hits
}

test('vault a passphrase: su disco nessun segreto in chiaro', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'm4-i06-'))
  const mnemonic = keys.generateMnemonic()
  const seed = keys.seedFromMnemonic(mnemonic)
  const roleKeys = keys.ROLES.map(r => keys.deriveRoleKey(seed, r))
  for (const k of roleKeys) {
    fs.writeFileSync(path.join(dir, `${k.role}.vault.json`), keys.sealSecret(keys.exportSecret(k), 'passphrase di prova lunga'))
  }
  fs.writeFileSync(path.join(dir, 'public.json'), JSON.stringify(roleKeys))
  const needles = [...roleKeys.flatMap(k => encodings(keys.exportSecret(k))), ...encodings(seed), mnemonic, mnemonic.split(' ').slice(0, 6).join(' ')]
  assert.deepEqual(scanDir(dir, needles), [])
})

test('il vault si riapre solo con la passphrase giusta', () => {
  const k = keys.deriveRoleKey(keys.seedFromMnemonic(keys.generateMnemonic()), 'identity')
  const sealed = keys.sealSecret(keys.exportSecret(k), 'giusta')
  assert.deepEqual(Buffer.from(keys.openSecret(sealed, 'giusta')), Buffer.from(keys.exportSecret(k)))
  assert.throws(() => keys.openSecret(sealed, 'sbagliata'))
})

test('un oggetto chiave serializzato o stampato non contiene il segreto', () => {
  const k = keys.deriveRoleKey(keys.seedFromMnemonic(keys.generateMnemonic()), 'wallet')
  const secretHex = Buffer.from(keys.exportSecret(k)).toString('hex')
  const outputs = [JSON.stringify(k), String(k), require('util').inspect(k, { showHidden: true, depth: 5 })]
  for (const o of outputs) assert.ok(!o.includes(secretHex), 'segreto trapelato: ' + o.slice(0, 80))
})

test('scrypt: il vault usa almeno N=2^17, r=8, p=1', () => {
  const o = JSON.parse(keys.sealSecret(new Uint8Array(32).fill(7), 'x'))
  assert.equal(o.kdf, 'scrypt')
  assert.ok(o.N >= 131072 && o.r >= 8 && o.p >= 1, `parametri deboli: N=${o.N} r=${o.r} p=${o.p}`)
})

test('scrypt: un vault con parametri deboli viene rifiutato in apertura', () => {
  const o = JSON.parse(keys.sealSecret(new Uint8Array(32).fill(7), 'x'))
  for (const weak of [{ N: 2 ** 14 }, { r: 1 }, { p: 0 }, { N: '131072' }]) {
    assert.throws(() => keys.openSecret(JSON.stringify({ ...o, ...weak }), 'x'), /scrypt/)
  }
})
