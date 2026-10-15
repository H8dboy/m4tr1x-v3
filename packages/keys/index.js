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
 * - Due famiglie di firme (ADR-0017, I-40): Schnorr secp256k1 per il social,
 *   ML-DSA-65 per ciò che vale. Questo pacchetto è l'unico che le conosce.
 */
const crypto = require('crypto')
const { schnorr, secp256k1 } = require('@noble/curves/secp256k1.js')
const { hkdf } = require('@noble/hashes/hkdf.js')
const { sha256 } = require('@noble/hashes/sha2.js')
const bip39 = require('@scure/bip39')
const { wordlist } = require('@scure/bip39/wordlists/english.js')
const { ml_dsa65 } = require('@noble/post-quantum/ml-dsa.js')

/** Ruoli ricavati dal seme dell'utente (KEYS.md: K1, K1d, K1r, K2). */
const ROLES = Object.freeze(['identity', 'dm', 'revoke', 'wallet'])

/** Chiave del nodo (K5): NON ricavata dal seme di un utente, generata a caso per ogni nodo. */
const NODE_ROLE = 'node'

/** Ruoli di sistema, generati a caso (mai dal seme di un utente). */
const SYSTEM_ROLES = Object.freeze(['node', 'issuer-root', 'issuer-op', 'release'])

/**
 * Famiglia di firma per ruolo (ADR-0017):
 *   social → Schnorr secp256k1 (Nostr) · valore → ML-DSA-65 (post-quantum)
 */
const FAMILY = Object.freeze({
  identity: 'schnorr', dm: 'schnorr', revoke: 'schnorr', node: 'schnorr',
  wallet: 'ml-dsa-65', 'issuer-root': 'ml-dsa-65', 'issuer-op': 'ml-dsa-65', release: 'ml-dsa-65',
})

/** Cosa può firmare ogni ruolo. Gli scopi sono i domini di @m4tr1x/protocol più "nostr-event". */
const PERMISSIONS = Object.freeze({
  node: Object.freeze(['nostr-event']),        // il protocollo la limita alle schede di directory
  identity: Object.freeze(['nostr-event', 'identity/migrate', 'dm/announce']),
  dm: Object.freeze([]),                       // la chiave DM decifra, non firma
  revoke: Object.freeze(['identity/revoke']),
  wallet: Object.freeze(['h8/spend', 'h8/rekey']),
  'issuer-root': Object.freeze([]),            // domini di conio e certificazione: fase F7
  'issuer-op': Object.freeze([]),              // domini delle righe del registro: fase F7
  release: Object.freeze(['release/manifest']),
})

const HKDF_SALT = new TextEncoder().encode('m4tr1x/v3/keys')
const CURVE_N = secp256k1.Point.Fn.ORDER

/** @type {WeakMap<RoleKey, Uint8Array>} */
const SECRETS = new WeakMap()

/** @param {string} role */
function familyOf(role) {
  const f = FAMILY[/** @type {keyof typeof FAMILY} */ (role)]
  if (!f) throw new Error(`ruolo sconosciuto: ${role}`)
  return f
}

class RoleKey {
  /**
   * @param {string} role @param {number} version
   * @param {Uint8Array} secret  Schnorr: chiave privata (32 byte) · ML-DSA: seme di 32 byte
   */
  constructor(role, version, secret) {
    /** @readonly */ this.role = role
    /** @readonly */ this.version = version
    /** @readonly */ this.family = familyOf(role)
    if (!(secret instanceof Uint8Array) || secret.length !== 32) throw new Error('segreto di 32 byte richiesto')
    /** @readonly hex: Schnorr x-only (32 byte, formato Nostr) · ML-DSA-65 (1952 byte) */
    this.publicKey = this.family === 'schnorr'
      ? Buffer.from(schnorr.getPublicKey(secret)).toString('hex')
      : Buffer.from(ml_dsa65.keygen(secret).publicKey).toString('hex')
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
    if (familyOf(role) === 'ml-dsa-65') return new RoleKey(role, version, sk)   // qualsiasi seme va bene
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
 * Firma di un digest da 32 byte con la famiglia del ruolo, solo se il ruolo può
 * firmare `purpose`. Usata da @m4tr1x/protocol: non chiamarla dall'app.
 * @param {RoleKey} key @param {Uint8Array} digest @param {string} purpose
 */
function signDigest(key, digest, purpose) {
  const s = SECRETS.get(key)
  if (!s) throw new Error('non è una RoleKey')
  if (!PERMISSIONS[/** @type {keyof typeof PERMISSIONS} */ (key.role)]?.includes(purpose))
    throw new Error(`il ruolo "${key.role}" non può firmare "${purpose}"`)
  if (!(digest instanceof Uint8Array) || digest.length !== 32) throw new Error('digest di 32 byte richiesto')
  if (key.family === 'schnorr') return Buffer.from(schnorr.sign(digest, s)).toString('hex')
  const { secretKey } = ml_dsa65.keygen(s)
  try { return Buffer.from(ml_dsa65.sign(digest, secretKey)).toString('hex') }
  finally { secretKey.fill(0) }
}

/**
 * Verifica una firma con la famiglia indicata. Rifiuta chiavi o firme della forma sbagliata.
 * @param {'schnorr'|'ml-dsa-65'} family @param {string} pubHex @param {string} sigHex @param {Uint8Array} digest
 */
function verifyDigest(family, pubHex, sigHex, digest) {
  try {
    if (family === 'schnorr') {
      if (!/^[0-9a-f]{64}$/.test(pubHex) || !/^[0-9a-f]{128}$/.test(sigHex)) return false
      return schnorr.verify(Buffer.from(sigHex, 'hex'), digest, Buffer.from(pubHex, 'hex'))
    }
    if (family === 'ml-dsa-65') {
      if (pubHex.length !== 1952 * 2 || sigHex.length !== 3309 * 2 || !/^[0-9a-f]+$/.test(pubHex + sigHex)) return false
      return ml_dsa65.verify(Buffer.from(sigHex, 'hex'), digest, Buffer.from(pubHex, 'hex'))
    }
    return false
  } catch { return false }
}

// ─── Vault a passphrase (quando il portachiavi del sistema non c'è) ──────────
// scrypt (incluso in Node, nessuna dipendenza) + AES-256-GCM. Parametri MINIMI
// N=2^17, r=8, p=1 (~128 MB, ~0,5 s): non esiste una variante più debole, e un
// vault con parametri sotto il minimo viene rifiutato anche in apertura.
const SCRYPT_MIN = Object.freeze({ N: 2 ** 17, r: 8, p: 1 })

/** @param {Uint8Array} secret @param {string} passphrase */
function sealSecret(secret, passphrase) {
  if (typeof passphrase !== 'string' || passphrase.length < 1) throw new Error('passphrase richiesta')
  const { N, r, p } = SCRYPT_MIN
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
  if (!(Number.isInteger(o.N) && Number.isInteger(o.r) && Number.isInteger(o.p)) ||
      o.N < SCRYPT_MIN.N || o.r < SCRYPT_MIN.r || o.p < SCRYPT_MIN.p || o.N > 2 ** 22 || o.r > 32 || o.p > 16)
    throw new Error('vault con parametri scrypt fuori dai limiti (minimo N=2^17, r=8, p=1)')
  const key = crypto.scryptSync(String(passphrase).normalize('NFKC'), Buffer.from(o.salt, 'base64'), 32, { N: o.N, r: o.r, p: o.p, maxmem: 256 * 1024 * 1024 })
  const d = crypto.createDecipheriv('aes-256-gcm', key, Buffer.from(o.iv, 'base64'))
  d.setAAD(Buffer.from('m4tr1x/vault/v1'))
  d.setAuthTag(Buffer.from(o.tag, 'base64'))
  try { return new Uint8Array(Buffer.concat([d.update(Buffer.from(o.ct, 'base64')), d.final()])) }
  catch { throw new Error('passphrase sbagliata o vault alterato') }
}

/** Ricostruisce una RoleKey da un segreto sigillato. @param {string} role @param {number} version @param {Uint8Array} secret */
function roleKeyFromSecret(role, version, secret) {
  familyOf(role)
  return new RoleKey(role, version, secret)
}

/**
 * Nuova chiave casuale per un ruolo di sistema (nodo, emittente, release).
 * @param {string} role
 */
function generateKey(role) {
  if (!SYSTEM_ROLES.includes(role)) throw new Error(`"${role}" non è un ruolo di sistema: le chiavi utente vengono dal seme`)
  for (;;) {
    const sk = crypto.randomBytes(32)
    if (familyOf(role) === 'ml-dsa-65') return new RoleKey(role, 1, sk)
    const n = BigInt('0x' + sk.toString('hex'))
    if (n > 0n && n < CURVE_N) return new RoleKey(role, 1, sk)
  }
}

/** Nuova chiave di nodo (K5), casuale. */
function generateNodeKey() {
  return generateKey(NODE_ROLE)
}

module.exports = {
  ROLES, NODE_ROLE, SYSTEM_ROLES, FAMILY, PERMISSIONS, SCRYPT_MIN, RoleKey, generateNodeKey, generateKey, verifyDigest,
  generateMnemonic, validateMnemonic, seedFromMnemonic, deriveRoleKey,
  exportSecret, signDigest, sealSecret, openSecret, roleKeyFromSecret,
}
