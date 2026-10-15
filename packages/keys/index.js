// @ts-check
'use strict'
/**
 * @m4tr1x/keys — seme, chiavi per ruolo, vault a passphrase.
 *
 * - Un seme di 24 parole (BIP-39) è l'unico backup dell'utente.
 * - Dal seme si ricava una chiave per ogni ruolo con HKDF-SHA256 ed etichette
 *   diverse (`m4tr1x/<ruolo>/v<n>`): chiavi indipendenti, un solo backup (ADR-0008).
 * - Il segreto di una chiave non è mai una proprietà dell'oggetto: sta in una
 *   WeakMap e si legge solo con `exportSecret`, così non finisce per sbaglio in
 *   un JSON, in un log o in una stampa (I-06).
 * - Ogni ruolo può firmare solo i propri scopi (I-07).
 */
const crypto = require('crypto')
const { schnorr, secp256k1 } = require('@noble/curves/secp256k1.js')
const { hkdf } = require('@noble/hashes/hkdf.js')
const { sha256 } = require('@noble/hashes/sha2.js')
const bip39 = require('@scure/bip39')
const { wordlist } = require('@scure/bip39/wordlists/english.js')

/** Ruoli ricavati dal seme dell'utente (KEYS.md: K1, K1d, K1r, K2). */
const ROLES = Object.freeze(['identity', 'dm', 'revoke', 'wallet'])

/** Cosa può firmare ogni ruolo. Gli scopi sono i domini di @m4tr1x/protocol più "nostr-event". */
const PERMISSIONS = Object.freeze({
  identity: Object.freeze(['nostr-event', 'identity/migrate', 'dm/announce']),
  dm: Object.freeze([]),                       // la chiave DM decifra, non firma
  revoke: Object.freeze(['identity/revoke']),
  wallet: Object.freeze(['h8/spend', 'h8/rekey']),
})

const HKDF_SALT = new TextEncoder().encode('m4tr1x/v3/keys')
const CURVE_N = secp256k1.Point.Fn.ORDER

/** @type {WeakMap<RoleKey, Uint8Array>} */
const SECRETS = new WeakMap()

class RoleKey {
  /** @param {string} role @param {number} version @param {Uint8Array} secret */
  constructor(role, version, secret) {
    /** @readonly */ this.role = role
    /** @readonly */ this.version = version
    /** @readonly x-only, hex (formato Nostr) */
    this.publicKey = Buffer.from(schnorr.getPublicKey(secret)).toString('hex')
    SECRETS.set(this, Uint8Array.from(secret))
    Object.freeze(this)
  }
  toJSON() { return { role: this.role, version: this.version, publicKey: this.publicKey } }
  toString() { return `RoleKey(${this.role} v${this.version} ${this.publicKey.slice(0, 12)}…)` }
  [Symbol.for('nodejs.util.inspect.custom')]() { return this.toString() }
}

/** 24 parole (256 bit di entropia). */
function generateMnemonic() {
  return bip39.generateMnemonic(wordlist, 256)
}

/** @param {string} mnemonic */
function validateMnemonic(mnemonic) {
  return bip39.validateMnemonic(String(mnemonic).trim().toLowerCase(), wordlist)
}

/**
 * Seme di 64 byte dalle parole (BIP-39 standard). La passphrase BIP-39 è facoltativa.
 * @param {string} mnemonic @param {string} [passphrase]
 */
function seedFromMnemonic(mnemonic, passphrase = '') {
  const m = String(mnemonic).trim().toLowerCase().split(/\s+/).join(' ')
  if (!validateMnemonic(m)) throw new Error('parole del seme non valide')
  return bip39.mnemonicToSeedSync(m, passphrase)
}

/**
 * Chiave del ruolo `role`, versione `version`, ricavata dal seme.
 * @param {Uint8Array} seed @param {string} role @param {number} [version]
 */
function deriveRoleKey(seed, role, version = 1) {
  if (!ROLES.includes(role)) throw new Error(`ruolo sconosciuto: ${role}`)
  if (!(seed instanceof Uint8Array) || seed.length < 32) throw new Error('seme non valido')
  if (!Number.isInteger(version) || version < 1) throw new Error('versione non valida')
  for (let counter = 0; counter < 256; counter++) {
    const info = new TextEncoder().encode(`m4tr1x/${role}/v${version}` + (counter ? `/${counter}` : ''))
    const sk = hkdf(sha256, seed, HKDF_SALT, info, 32)
    const n = BigInt('0x' + Buffer.from(sk).toString('hex'))
    if (n > 0n && n < CURVE_N) return new RoleKey(role, version, sk)
  }
  throw new Error('derivazione impossibile')   // probabilità ~2^-32000
}

/** Il segreto di una chiave: solo per sigillarlo nel vault o nel portachiavi del sistema. @param {RoleKey} key */
function exportSecret(key) {
  const s = SECRETS.get(key)
  if (!s) throw new Error('non è una RoleKey')
  return Uint8Array.from(s)
}

/**
 * Firma Schnorr di un digest da 32 byte, solo se il ruolo può firmare `purpose`.
 * Usata da @m4tr1x/protocol: non chiamarla direttamente dall'app.
 * @param {RoleKey} key @param {Uint8Array} digest @param {string} purpose
 */
function signDigest(key, digest, purpose) {
  const s = SECRETS.get(key)
  if (!s) throw new Error('non è una RoleKey')
  if (!PERMISSIONS[/** @type {keyof typeof PERMISSIONS} */ (key.role)]?.includes(purpose))
    throw new Error(`il ruolo "${key.role}" non può firmare "${purpose}"`)
  if (!(digest instanceof Uint8Array) || digest.length !== 32) throw new Error('digest di 32 byte richiesto')
  return Buffer.from(schnorr.sign(digest, s)).toString('hex')
}

// ─── Vault a passphrase (quando il portachiavi del sistema non c'è) ──────────
// scrypt (incluso in Node, nessuna dipendenza) + AES-256-GCM. Parametri: N=2^17,
// r=8, p=1 (~128 MB, ~0,5 s). `fast` solo nei test.

/** @param {Uint8Array} secret @param {string} passphrase @param {{fast?: boolean}} [opts] */
function sealSecret(secret, passphrase, opts = {}) {
  if (typeof passphrase !== 'string' || passphrase.length < 1) throw new Error('passphrase richiesta')
  const N = opts.fast ? 2 ** 14 : 2 ** 17, r = 8, p = 1
  const salt = crypto.randomBytes(16)
  const key = crypto.scryptSync(passphrase.normalize('NFKC'), salt, 32, { N, r, p, maxmem: 256 * 1024 * 1024 })
  const iv = crypto.randomBytes(12)
  const c = crypto.createCipheriv('aes-256-gcm', key, iv)
  c.setAAD(Buffer.from('m4tr1x/vault/v1'))
  const ct = Buffer.concat([c.update(secret), c.final()])
  return JSON.stringify({ v: 1, kdf: 'scrypt', N, r, p, salt: salt.toString('base64'), iv: iv.toString('base64'), tag: c.getAuthTag().toString('base64'), ct: ct.toString('base64') })
}

/** @param {string} sealed @param {string} passphrase */
function openSecret(sealed, passphrase) {
  const o = JSON.parse(sealed)
  if (o.v !== 1 || o.kdf !== 'scrypt') throw new Error('formato del vault sconosciuto')
  const key = crypto.scryptSync(String(passphrase).normalize('NFKC'), Buffer.from(o.salt, 'base64'), 32, { N: o.N, r: o.r, p: o.p, maxmem: 256 * 1024 * 1024 })
  const d = crypto.createDecipheriv('aes-256-gcm', key, Buffer.from(o.iv, 'base64'))
  d.setAAD(Buffer.from('m4tr1x/vault/v1'))
  d.setAuthTag(Buffer.from(o.tag, 'base64'))
  try { return new Uint8Array(Buffer.concat([d.update(Buffer.from(o.ct, 'base64')), d.final()])) }
  catch { throw new Error('passphrase sbagliata o vault alterato') }
}

/** Ricostruisce una RoleKey da un segreto sigillato. @param {string} role @param {number} version @param {Uint8Array} secret */
function roleKeyFromSecret(role, version, secret) {
  if (!ROLES.includes(role)) throw new Error(`ruolo sconosciuto: ${role}`)
  return new RoleKey(role, version, secret)
}

module.exports = {
  ROLES, PERMISSIONS, RoleKey,
  generateMnemonic, validateMnemonic, seedFromMnemonic, deriveRoleKey,
  exportSecret, signDigest, sealSecret, openSecret, roleKeyFromSecret,
}
