// @ts-check
'use strict'
/**
 * @m4tr1x/protocol — eventi Nostr e firme con etichetta di dominio.
 *
 * - Eventi: NIP-01 (id = sha256 della serializzazione, firma Schnorr), con in
 *   più un tag obbligatorio ["net", <rete>]: un evento della rete di prova non
 *   vale nella rete principale (I-08).
 * - Firme non-Nostr (spese H8, revoche, migrazioni…): digest =
 *   sha256("m4tr1x/<dominio>/v1\n<rete>\n" || payload). Il dominio e la rete
 *   fanno parte di ciò che si firma (I-08).
 * - Nessun dato H8 (tag o id di conto) può entrare in un evento (I-15).
 */
const crypto = require('crypto')
const { schnorr } = require('@noble/curves/secp256k1.js')
const { sha256 } = require('@noble/hashes/sha2.js')
const keys = require('../keys')

const NETWORKS = Object.freeze(['m4tr1x-main', 'm4tr1x-test'])
/** Domini delle firme non-Nostr. Il ruolo ammesso per ognuno è in @m4tr1x/keys PERMISSIONS. */
const DOMAINS = Object.freeze(['h8/spend', 'h8/rekey', 'identity/migrate', 'identity/revoke', 'dm/announce'])

// ─── Dati H8 vietati negli eventi (I-15) ─────────────────────────────────────
/** Formato degli id di conto H8 v3: "m4a1" + 32 esadecimali casuali. */
const ACCOUNT_ID_RE = /\bm4a1[0-9a-f]{32}\b/i
/** Indirizzi H8 del vecchio sistema (repo v2): non devono comparire nemmeno per sbaglio. */
const LEGACY_H8_RE = /\bH8[0-9a-f]{38}\b/i
const FORBIDDEN_TAGS = new Set(['h8', 'h8host', 'wallet', 'account', 'h8account'])

/** Nuovo id di conto casuale (128 bit), non derivabile da nessuna chiave (I-14). */
function newAccountId() {
  return 'm4a1' + crypto.randomBytes(16).toString('hex')
}

/** @param {{content: string, tags: string[][]}} ev @returns {string|null} motivo, o null se pulito */
function h8Violation(ev) {
  for (const t of ev.tags) {
    if (FORBIDDEN_TAGS.has(String(t[0]).toLowerCase())) return `tag "${t[0]}" vietato (dati H8)`
    for (const v of t) if (ACCOUNT_ID_RE.test(v) || LEGACY_H8_RE.test(v)) return 'conto o indirizzo H8 in un tag'
  }
  if (ACCOUNT_ID_RE.test(ev.content) || LEGACY_H8_RE.test(ev.content)) return 'conto o indirizzo H8 nel contenuto'
  return null
}

// ─── Firme con dominio ───────────────────────────────────────────────────────
/** @param {string} domain @param {string} network @param {Uint8Array} payload */
function domainDigest(domain, network, payload) {
  if (!DOMAINS.includes(domain)) throw new Error(`dominio sconosciuto: ${domain}`)
  if (!NETWORKS.includes(network)) throw new Error(`rete sconosciuta: ${network}`)
  if (!(payload instanceof Uint8Array)) throw new Error('payload in byte richiesto')
  const head = new TextEncoder().encode(`m4tr1x/${domain}/v1\n${network}\n`)
  const msg = new Uint8Array(head.length + payload.length)
  msg.set(head); msg.set(payload, head.length)
  return sha256(msg)
}

/** @param {import('../keys').RoleKey} key @param {string} domain @param {string} network @param {Uint8Array} payload */
function signDomain(key, domain, network, payload) {
  return keys.signDigest(key, domainDigest(domain, network, payload), domain)
}

/** @param {string} pubkeyHex @param {string} sigHex @param {string} domain @param {string} network @param {Uint8Array} payload */
function verifyDomain(pubkeyHex, sigHex, domain, network, payload) {
  try {
    if (!/^[0-9a-f]{64}$/.test(pubkeyHex) || !/^[0-9a-f]{128}$/.test(sigHex)) return false
    return schnorr.verify(Buffer.from(sigHex, 'hex'), domainDigest(domain, network, payload), Buffer.from(pubkeyHex, 'hex'))
  } catch { return false }
}

// ─── Eventi Nostr ────────────────────────────────────────────────────────────
/** @typedef {{ id: string, pubkey: string, created_at: number, kind: number, tags: string[][], content: string, sig: string }} NostrEvent */

/** @param {string} pubkey @param {number} created_at @param {number} kind @param {string[][]} tags @param {string} content */
function eventId(pubkey, created_at, kind, tags, content) {
  return Buffer.from(sha256(new TextEncoder().encode(JSON.stringify([0, pubkey, created_at, kind, tags, content])))).toString('hex')
}

/** @param {{kind: number, content: string, tags: string[][], created_at: number}} t @param {import('../keys').RoleKey} key */
function signRaw(t, key) {
  const id = eventId(key.publicKey, t.created_at, t.kind, t.tags, t.content)
  const sig = keys.signDigest(key, Buffer.from(id, 'hex'), 'nostr-event')
  return { id, pubkey: key.publicKey, created_at: t.created_at, kind: t.kind, tags: t.tags, content: t.content, sig }
}

/** @param {any} t */
function checkTemplate(t) {
  if (!t || typeof t !== 'object') throw new Error('evento non valido')
  if (!Number.isInteger(t.kind) || t.kind < 0 || t.kind > 65535) throw new Error('kind non valido')
  if (typeof t.content !== 'string') throw new Error('content deve essere una stringa')
  if (!Array.isArray(t.tags) || !t.tags.every(/** @param {any} x */ x => Array.isArray(x) && x.length > 0 && x.every(y => typeof y === 'string')))
    throw new Error('tags non validi')
}

/**
 * Costruisce e firma un evento con la chiave identità. Aggiunge il tag di rete.
 * @param {{kind: number, content: string, tags: string[][], created_at?: number}} template
 * @param {import('../keys').RoleKey} identityKey
 * @param {string} network
 * @returns {NostrEvent}
 */
function buildEvent(template, identityKey, network) {
  if (!NETWORKS.includes(network)) throw new Error(`rete sconosciuta: ${network}`)
  checkTemplate(template)
  if (template.tags.some(t => t[0] === 'net')) throw new Error('il tag "net" lo aggiunge il protocollo')
  const ev = {
    kind: template.kind,
    content: template.content,
    tags: [...template.tags.map(t => [...t]), ['net', network]],
    created_at: template.created_at ?? Math.floor(Date.now() / 1000),
  }
  const why = h8Violation(ev)
  if (why) throw new Error(`H8: ${why}`)
  return signRaw(ev, identityKey)
}

/**
 * Verifica completa di un evento ricevuto: forma, id, firma, rete, assenza di dati H8.
 * @param {any} ev @param {string} network
 */
function verifyEvent(ev, network) {
  try {
    if (!NETWORKS.includes(network)) return false
    if (!ev || typeof ev !== 'object') return false
    if (!/^[0-9a-f]{64}$/.test(ev.id) || !/^[0-9a-f]{64}$/.test(ev.pubkey) || !/^[0-9a-f]{128}$/.test(ev.sig)) return false
    if (!Number.isInteger(ev.created_at) || ev.created_at < 0) return false
    checkTemplate(ev)
    const nets = ev.tags.filter(/** @param {string[]} t */ t => t[0] === 'net')
    if (nets.length !== 1 || nets[0][1] !== network) return false
    if (h8Violation(ev)) return false
    if (eventId(ev.pubkey, ev.created_at, ev.kind, ev.tags, ev.content) !== ev.id) return false
    return schnorr.verify(Buffer.from(ev.sig, 'hex'), Buffer.from(ev.id, 'hex'), Buffer.from(ev.pubkey, 'hex'))
  } catch { return false }
}

/** Solo per i test: firma un evento SENZA le regole del protocollo (per verificare che verifyEvent lo rifiuti). */
function _signRawEventForTest(/** @type {any} */ t, /** @type {import('../keys').RoleKey} */ key) {
  return signRaw(t, key)
}

module.exports = {
  NETWORKS, DOMAINS, ACCOUNT_ID_RE,
  newAccountId, domainDigest, signDomain, verifyDomain,
  eventId, buildEvent, verifyEvent, _signRawEventForTest,
}
