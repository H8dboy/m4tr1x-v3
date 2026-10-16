// @ts-check
'use strict'
/**
 * I-40 — Due famiglie di firme (ADR-0017):
 *   Schnorr per tutto il social (identità, DM, revoca, nodo), ML-DSA-65 per tutto
 *   ciò che vale (wallet, emittente, release). Ed25519 solo dentro Tor, RSA solo
 *   nel modulo buoni.
 * - ogni ruolo ha la sua famiglia, e un solo pacchetto la decide
 * - una firma Schnorr non vale mai per un dominio di valore, e viceversa
 * - statico: nessun uso di Ed25519/RSA/minisign nel codice fuori dai recinti
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const keys = require('../../packages/keys')
const protocol = require('../../packages/protocol')

const NET = 'm4tr1x-test'
const seed = keys.seedFromMnemonic(keys.generateMnemonic())

test('famiglia per ruolo', () => {
  for (const r of ['identity', 'dm', 'revoke', 'node']) assert.equal(keys.FAMILY[r], 'schnorr', r)
  for (const r of ['wallet', 'issuer-root', 'issuer-op', 'release']) assert.equal(keys.FAMILY[r], 'ml-dsa-65', r)
})

test('le chiavi hanno la forma della loro famiglia', () => {
  assert.equal(keys.deriveRoleKey(seed, 'identity').publicKey.length, 64)            // 32 byte x-only
  assert.equal(keys.deriveRoleKey(seed, 'wallet').publicKey.length, 1952 * 2)        // ML-DSA-65
  assert.equal(keys.generateKey('release').publicKey.length, 1952 * 2)
  assert.equal(keys.generateKey('node').publicKey.length, 64)
})

test('firma del wallet: ML-DSA-65, verificata come tale', () => {
  const w = keys.deriveRoleKey(seed, 'wallet')
  const payload = new TextEncoder().encode('{"importo":100,"seq":1}')
  const sig = protocol.signDomain(w, 'h8/spend', NET, payload)
  assert.equal(sig.length, 3309 * 2)
  assert.ok(protocol.verifyDomain(w.publicKey, sig, 'h8/spend', NET, payload))
})

test('una firma Schnorr non vale mai per un dominio di valore', () => {
  const id = keys.deriveRoleKey(seed, 'identity')
  const payload = new TextEncoder().encode('x')
  // firma Schnorr valida sullo stesso digest, presentata come spesa H8
  const digest = protocol.domainDigest('h8/spend', NET, payload)
  const schnorrSig = require('@noble/curves/secp256k1.js').schnorr.sign(digest, keys.exportSecret(id))
  assert.equal(protocol.verifyDomain(id.publicKey, Buffer.from(schnorrSig).toString('hex'), 'h8/spend', NET, payload), false)
  assert.equal(protocol.verifyDomain(id.publicKey, Buffer.from(schnorrSig).toString('hex'), 'release/manifest', NET, payload), false)
})

test('una firma ML-DSA non vale mai per un dominio social', () => {
  const w = keys.deriveRoleKey(seed, 'wallet')
  const payload = new TextEncoder().encode('x')
  const digest = protocol.domainDigest('identity/migrate', NET, payload)
  const { ml_dsa65 } = require('@noble/post-quantum/ml-dsa.js')
  const sk = ml_dsa65.keygen(keys.exportSecret(w)).secretKey
  const sig = Buffer.from(ml_dsa65.sign(digest, sk)).toString('hex')
  assert.equal(protocol.verifyDomain(w.publicKey, sig, 'identity/migrate', NET, payload), false)
})

test('statico: Ed25519, RSA e minisign non compaiono nel codice fuori dai recinti', () => {
  const root = path.join(__dirname, '..', '..')
  const walk = (/** @type {string} */ d) => fs.readdirSync(d, { withFileTypes: true }).flatMap(e =>
    e.name === 'node_modules' || e.name.startsWith('.') ? [] : e.isDirectory() ? walk(path.join(d, e.name)) : [path.join(d, e.name)])
  const files = ['packages', 'node', 'tools'].flatMap(d => fs.existsSync(path.join(root, d)) ? walk(path.join(root, d)) : []).filter(f => f.endsWith('.js'))
  // recinti: il modulo buoni (RSA, F7b). Ed25519 non ha recinto nel nostro codice: è dentro Tor.
  const allowRsa = (/** @type {string} */ f) => f.includes(path.join('packages', 'vouchers'))
  // lo scanner dei segreti CERCA chiavi minisign trapelate: le nomina, non le usa
  const detectors = new Set([path.join(root, 'tools', 'security', 'scan-secrets.js')])
  // eccezione stretta (16/10/2026): tor.js LEGGE i byte del certificato del descrittore che tor
  // stesso produce (etichetta PEM "ED25519 CERT"), per sapere quale descrittore è l'attuale.
  // Nessuna operazione Ed25519: si tolgono solo le due etichette, e il file non deve importare
  // nessuna libreria Ed25519.
  const torJs = path.join(root, 'node', 'src', 'tor.js')
  // e passa a tor la chiave dell'onion come blob opaco con l'etichetta di tipo "ED25519-V3" (ADD_ONION)
  const etichette = (/** @type {string} */ f, /** @type {string} */ s) => f === torJs ? s.replace(/-----(BEGIN|END) ED25519 CERT-----/g, '').replace(/\bED25519-V3\b/g, '') : s
  if (fs.existsSync(torJs)) assert.ok(!/require\([^)]*(ed25519|curves|tweetnacl|sodium)/i.test(fs.readFileSync(torJs, 'utf8')), 'tor.js importa una libreria Ed25519')
  for (const f of files.filter(x => !detectors.has(x))) {
    const src = etichette(f, fs.readFileSync(f, 'utf8').replace(/\/\/.*$|\/\*[\s\S]*?\*\//gm, ''))
    assert.ok(!/ed25519|minisign/i.test(src), `Ed25519/minisign nel codice: ${path.relative(root, f)}`)
    if (!allowRsa(f)) assert.ok(!/['"`]rsa['"`]|rsa-pss|RSA_PKCS|createSign\(/i.test(src), `RSA fuori dal modulo buoni: ${path.relative(root, f)}`)
  }
})
