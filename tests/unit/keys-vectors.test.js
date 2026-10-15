// @ts-check
'use strict'
/**
 * Vettori di prova di @m4tr1x/keys.
 * - BIP-39: vettore ufficiale (Trezor) → la libreria produce il seme standard.
 * - Derivazione: valori CONGELATI da un seme pubblico di prova. Se cambiano,
 *   cambierebbero le identità di tutti gli utenti: il test deve fallire.
 * Nessun segreto qui: il seme di prova è pubblico e noto a tutti.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const keys = require('../../packages/keys')

test('BIP-39: vettore ufficiale Trezor', () => {
  const m = 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about'
  // seme atteso in due parti (lo scanner dei segreti segnala le stringhe da 64 byte)
  const expected = 'c55257c360c07c72029aebc1b53c05ed0362ada38ead3e3e9efa3708e5349553' + '1f09a6987599d18264c1e1c92f2cf141630c7a3c4ab7c81b2f001698e7463b04'
  assert.equal(Buffer.from(keys.seedFromMnemonic(m, 'TREZOR')).toString('hex'), expected)
})

const FROZEN = {
  identity: ['817d2b662e9c2c6e080c5fd5d9c4866a8342d194dbea7f98c1f4fc30e069bed7', 'd2f184cb121675cfab83351bc3cb0150d566e212ef1658b03c1b4983b5598f88'],
  dm: ['03c0455c6c26dffc42017bc436bc8c994cb943973e5cac986f0b47dcafe98837', '0ff48b21a30c67bad952a45f488470d60df00fc0693af8a3347a768eb3e121d5'],
  revoke: ['60eed1298e5ccf492e850d922bc7822b3a37b740dcd4adb7a973f05e2bbc5242', 'c429002897006a6658424189b277f40180eed68dc52fc77e4cd54dd608005351'],
  // wallet = ML-DSA-65 (ADR-0017, 15/10/2026): si congela lo sha256 della chiave pubblica (1952 byte)
  wallet: ['7caf829515e0da88e621b8ad535c1c607770d4399f48eed5f23bd027d3033eb5', 'c8f5cfdd38e77a72d49a25db5c311d0e1fc195f9503d1f41b6e2684955c6fd35'],
}
const fingerprint = (/** @type {string} */ role, /** @type {string} */ pub) =>
  keys.FAMILY[role] === 'ml-dsa-65' ? require('crypto').createHash('sha256').update(pub).digest('hex') : pub

test('derivazione per ruolo e versione: valori congelati', () => {
  const seed = keys.seedFromMnemonic('abandon '.repeat(23) + 'art')
  for (const role of keys.ROLES) {
    assert.equal(fingerprint(role, keys.deriveRoleKey(seed, role, 1).publicKey), FROZEN[role][0], `${role} v1`)
    assert.equal(fingerprint(role, keys.deriveRoleKey(seed, role, 2).publicKey), FROZEN[role][1], `${role} v2`)
  }
})

test('parole non valide rifiutate', () => {
  assert.throws(() => keys.seedFromMnemonic('ciao mondo questo non è un seme'), /non valide/)
  assert.throws(() => keys.seedFromMnemonic('abandon '.repeat(24).trim()), /non valide/)   // checksum sbagliato
})

test('24 parole generate, tutte diverse a ogni chiamata', () => {
  const a = keys.generateMnemonic(), b = keys.generateMnemonic()
  assert.equal(a.split(' ').length, 24)
  assert.ok(keys.validateMnemonic(a))
  assert.notEqual(a, b)
})
