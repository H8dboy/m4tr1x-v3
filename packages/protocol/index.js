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
/**
 * Domini delle firme non-Nostr e famiglia con cui si verificano (ADR-0017, I-40):
 * ciò che vale → ML-DSA-65, il social → Schnorr. Il ruolo ammesso per ogni
 * dominio è in @m4tr1x/keys PERMISSIONS.
 */
const DOMAIN_FAMILY = Object.freeze({
  'h8/spend': 'ml-dsa-65', 'h8/rekey': 'ml-dsa-65', 'release/manifest': 'ml-dsa-65',
  'identity/migrate': 'schnorr', 'identity/revoke': 'schnorr', 'dm/announce': 'schnorr',
})
const DOMAINS = Object.freeze(Object.keys(DOMAIN_FAMILY))

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
  const digest = domainDigest(domain, network, payload)
  if (DOMAIN_FAMILY[/** @type {keyof typeof DOMAIN_FAMILY} */ (domain)] !== key.family)
    throw new Error(`il ruolo "${key.role}" (famiglia ${key.family}) non può firmare "${domain}"`)
  return keys.signDigest(key, digest, domain)
}

/**
 * Verifica con la famiglia del dominio: una firma Schnorr non vale mai per un
 * dominio di valore, né una ML-DSA per un dominio social.
 * @param {string} pubkeyHex @param {string} sigHex @param {string} domain @param {string} network @param {Uint8Array} payload
 */
function verifyDomain(pubkeyHex, sigHex, domain, network, payload) {
  try {
    const fam = DOMAIN_FAMILY[/** @type {keyof typeof DOMAIN_FAMILY} */ (domain)]
    if (!fam || typeof pubkeyHex !== 'string' || typeof sigHex !== 'string') return false
    return keys.verifyDigest(/** @type {any} */ (fam), pubkeyHex, sigHex, domainDigest(domain, network, payload))
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
  if (identityKey.family !== 'schnorr') throw new Error(`il ruolo "${identityKey.role}" non firma eventi Nostr`)
  if (identityKey.role === keys.NODE_ROLE && template.kind !== DESCRIPTOR_KIND)
    throw new Error('la chiave del nodo firma solo schede di directory')
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

// ─── Directory: schede dei nodi (ADR-0003, I-29, I-38) ─────────────────────────
/** Evento sostituibile parametrizzato (NIP-33): una sola scheda attiva per nodo. */
const DESCRIPTOR_KIND = 30480
const DESCRIPTOR_D = 'm4tr1x-node'
const MAX_DESCRIPTOR_TTL = 7 * 86400
const NODE_ROLES = Object.freeze(['user', 'edge', 'hub', 'anchor'])
const ONION_V3 = /^[a-z2-7]{56}\.onion$/
const ANY_IPV4 = /\b(?:\d{1,3}\.){3}\d{1,3}\b/
const ANY_IPV6 = /\b(?:[0-9a-f]{1,4}:){2,7}[0-9a-f]{0,4}\b|::1\b/i

/** Bit a zero iniziali dell'id (difficoltà NIP-13). @param {string} idHex */
function leadingZeroBits(idHex) {
  let bits = 0
  for (const ch of idHex) {
    const v = parseInt(ch, 16)
    if (v === 0) { bits += 4; continue }
    bits += Math.clz32(v) - 28
    break
  }
  return bits
}

/**
 * Scheda di un nodo, firmata con la sua chiave (K5), con prova di lavoro facoltativa.
 * @param {import('../keys').RoleKey} nodeKey
 * @param {string} network
 * @param {{ onion: string, roles: string[], version: string, ttlSec: number, powBits?: number, createdAt?: number }} o
 */
function buildDescriptor(nodeKey, network, o) {
  if (nodeKey.role !== keys.NODE_ROLE) throw new Error('una scheda la firma solo la chiave del nodo (ruolo "node")')
  if (!NETWORKS.includes(network)) throw new Error(`rete sconosciuta: ${network}`)
  if (!ONION_V3.test(String(o.onion))) throw new Error('la scheda accetta solo un indirizzo onion v3')
  if (!Array.isArray(o.roles) || !o.roles.length || !o.roles.every(r => NODE_ROLES.includes(r))) throw new Error('ruolo del nodo sconosciuto')
  if (!Number.isInteger(o.ttlSec) || o.ttlSec < 1 || o.ttlSec > MAX_DESCRIPTOR_TTL) throw new Error('scadenza della scheda: da 1 secondo a 7 giorni')
  const created_at = o.createdAt ?? Math.floor(Date.now() / 1000)
  const content = JSON.stringify({ roles: o.roles, version: String(o.version).slice(0, 32) })
  const base = [['d', DESCRIPTOR_D], ['onion', o.onion], ['expiration', String(created_at + o.ttlSec)]]
  const bits = o.powBits || 0
  let tags = [...base, ['net', network]]
  if (bits > 0) {
    for (let nonce = 0; ; nonce++) {
      const t = [...base, ['nonce', String(nonce), String(bits)], ['net', network]]
      if (leadingZeroBits(eventId(nodeKey.publicKey, created_at, DESCRIPTOR_KIND, t, content)) >= bits) { tags = t; break }
    }
  }
  const ev = { kind: DESCRIPTOR_KIND, content, tags, created_at }
  const why = h8Violation(ev)
  if (why) throw new Error(`H8: ${why}`)
  return signRaw(ev, nodeKey)
}

/**
 * Verifica completa di una scheda ricevuta.
 * @param {any} ev @param {string} network @param {number} now secondi
 * @returns {{ ok: true } | { ok: false, reason: string }}
 */
function verifyDescriptor(ev, network, now) {
  const no = (/** @type {string} */ reason) => ({ ok: /** @type {false} */ (false), reason })
  if (!verifyEvent(ev, network)) return no('firma, rete o forma non valide')
  if (ev.kind !== DESCRIPTOR_KIND) return no('non è una scheda di directory')
  const all = (/** @type {string} */ n) => ev.tags.filter((/** @type {string[]} */ t) => t[0] === n)
  if (all('d').length !== 1 || all('d')[0][1] !== DESCRIPTOR_D) return no('tag d mancante')
  const onions = all('onion')
  if (onions.length !== 1 || !ONION_V3.test(onions[0][1] || '')) return no('serve un solo indirizzo onion v3')
  const exp = all('expiration')
  const expAt = exp.length === 1 ? Number(exp[0][1]) : NaN
  if (!Number.isInteger(expAt)) return no('scadenza mancante')
  if (ev.created_at > now + 900) return no('data nel futuro')
  if (expAt <= now) return no('scheda scaduta')
  if (expAt - ev.created_at > MAX_DESCRIPTOR_TTL) return no('scadenza oltre 7 giorni')
  const haystack = ev.content + ' ' + ev.tags.map((/** @type {string[]} */ t) => t.join(' ')).join(' ')
  if (ANY_IPV4.test(haystack) || ANY_IPV6.test(haystack)) return no('indirizzo IP nella scheda')
  let body
  try { body = JSON.parse(ev.content) } catch { return no('contenuto non JSON') }
  if (!body || !Array.isArray(body.roles) || !body.roles.length || !body.roles.every((/** @type {string} */ r) => NODE_ROLES.includes(r))) return no('ruoli non validi')
  return { ok: true }
}

/** Solo per i test: firma un evento SENZA le regole del protocollo (per verificare che verifyEvent lo rifiuti). */
function _signRawEventForTest(/** @type {any} */ t, /** @type {import('../keys').RoleKey} */ key) {
  return signRaw(t, key)
}

module.exports = {
  NETWORKS, DOMAINS, DOMAIN_FAMILY, ACCOUNT_ID_RE,
  DESCRIPTOR_KIND, DESCRIPTOR_D, MAX_DESCRIPTOR_TTL, NODE_ROLES, leadingZeroBits, buildDescriptor, verifyDescriptor,
  newAccountId, domainDigest, signDomain, verifyDomain,
  eventId, buildEvent, verifyEvent, _signRawEventForTest,
}
