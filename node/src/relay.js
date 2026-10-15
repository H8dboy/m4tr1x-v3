// @ts-check
'use strict'
/**
 * Relay Nostr del nodo (NIP-01), portato dal nodo v2 irrobustito il 15/10/2026.
 * Tutto ciò che arriva dal socket è ostile finché non è normalizzato (I-26):
 * - frame ≤ 512 KB, ≤ 64 sottoscrizioni, ≤ 10 filtri per REQ, ≤ 500 elementi per lista
 * - limiti per connessione (token bucket) e globali (dietro Tor tutti sono 127.0.0.1)
 * - un evento si accetta solo se passa protocol.verifyEvent sulla rete del nodo
 *   (firma, id, tag di rete, nessun dato H8)
 * - eventi effimeri (kind 20000–29999) inoltrati, mai salvati
 */
const crypto = require('crypto')
const { WebSocketServer } = require('ws')
const protocol = require('../../packages/protocol')

const MAX_PAYLOAD = 512 * 1024
const MAX_SUBS = 64
const MAX_FILTERS = 10
const MAX_LIST = 500
const MAX_CONNECTIONS = 500
const EVENT_BURST = 30
const EVENT_REFILL_MS = 1000
const MAX_FUTURE_SEC = 15 * 60
const isEphemeral = (/** @type {number} */ k) => k >= 20000 && k < 30000
/** NIP-01: sostituibili (uno per pubkey+kind) e parametrizzati (uno per pubkey+kind+d). */
const isReplaceable = (/** @type {number} */ k) => k === 0 || k === 3 || (k >= 10000 && k < 20000)
const isParamReplaceable = (/** @type {number} */ k) => k >= 30000 && k < 40000
const nowSec = () => Math.floor(Date.now() / 1000)
/** @param {any} ev */
function expirationOf(ev) {
  const t = ev.tags.find((/** @type {string[]} */ x) => x[0] === 'expiration')
  const n = t ? Number(t[1]) : NaN
  return Number.isInteger(n) ? n : null
}

const HEX = /^[0-9a-f]{1,64}$/
const strList = (/** @type {any} */ v, /** @type {RegExp} [re] */ re = undefined) => Array.isArray(v) && v.length <= MAX_LIST && v.every(x => typeof x === 'string' && x.length <= 256 && (!re || re.test(x)))
const intList = (/** @type {any} */ v) => Array.isArray(v) && v.length <= MAX_LIST && v.every(x => Number.isInteger(x) && x >= 0 && x <= 65535)

/** @param {any} f @returns {Record<string, any> | null} */
function sanitizeFilter(f) {
  if (!f || typeof f !== 'object' || Array.isArray(f)) return null
  /** @type {Record<string, any>} */ const out = {}
  if (f.ids !== undefined) { if (!strList(f.ids, HEX)) return null; if (f.ids.length) out.ids = f.ids }
  if (f.authors !== undefined) { if (!strList(f.authors, HEX)) return null; if (f.authors.length) out.authors = f.authors }
  if (f.kinds !== undefined) { if (!intList(f.kinds)) return null; if (f.kinds.length) out.kinds = f.kinds }
  if (f.since !== undefined) { if (!Number.isInteger(f.since) || f.since < 0) return null; out.since = f.since }
  if (f.until !== undefined) { if (!Number.isInteger(f.until) || f.until < 0) return null; out.until = f.until }
  if (f.limit !== undefined) { if (!Number.isInteger(f.limit)) return null; out.limit = Math.max(1, Math.min(f.limit, 500)) }
  for (const [k, v] of Object.entries(f)) {
    if (!k.startsWith('#')) continue
    if (k.length !== 2 || !/^[a-zA-Z]$/.test(k[1]) || !strList(v)) return null
    if (v.length) out[k] = v
  }
  return out
}

/** @param {any} ev @param {Record<string, any>} f */
function matches(ev, f) {
  if (f.ids && !f.ids.some((/** @type {string} */ p) => ev.id.startsWith(p))) return false
  if (f.authors && !f.authors.some((/** @type {string} */ p) => ev.pubkey.startsWith(p))) return false
  if (f.kinds && !f.kinds.includes(ev.kind)) return false
  if (f.since && ev.created_at < f.since) return false
  if (f.until && ev.created_at > f.until) return false
  for (const [k, vals] of Object.entries(f)) {
    if (!k.startsWith('#')) continue
    if (!ev.tags.some((/** @type {string[]} */ t) => t[0] === k[1] && vals.includes(t[1]))) return false
  }
  return true
}

/**
 * @typedef {(ev: any, meta: { fromPeer: boolean }) => [boolean, string]} RelayPolicy
 *   regole aggiuntive del ruolo (es. ancora): chiamata dopo la verifica di base
 * @param {{ db: import('node:sqlite').DatabaseSync, network: string, log: (s: string) => void, policy?: RelayPolicy }} ctx
 */
function createRelay(ctx) {
  const { db, network, log } = ctx
  const insEv = db.prepare('INSERT OR IGNORE INTO events (id,pubkey,created_at,kind,tags,content,sig,received_at,expires_at,dtag) VALUES (?,?,?,?,?,?,?,?,?,?)')
  const findRepl = db.prepare('SELECT id, created_at FROM events WHERE pubkey = ? AND kind = ? AND dtag IS ?')
  const delEv = db.prepare('DELETE FROM events WHERE id = ?')
  const purgeStmt = db.prepare('DELETE FROM events WHERE expires_at IS NOT NULL AND expires_at <= ?')
  const insTag = db.prepare('INSERT INTO event_tags (event_id,name,value) VALUES (?,?,?)')
  const wss = new WebSocketServer({ noServer: true, maxPayload: MAX_PAYLOAD })
  /** @type {Map<string, { ws: import('ws').WebSocket, sid: string, filters: Record<string, any>[] }>} */
  const subs = new Map()
  /** @type {Set<(ev: any) => void>} */
  const listeners = new Set()

  /** Salva un evento già verificato; true se nuovo. Gestisce i sostituibili. @param {any} ev */
  function store(ev) {
    const repl = isReplaceable(ev.kind) || isParamReplaceable(ev.kind)
    const dtag = isParamReplaceable(ev.kind) ? String(ev.tags.find((/** @type {string[]} */ t) => t[0] === 'd')?.[1] ?? '') : null
    if (repl) {
      const old = /** @type {any} */ (findRepl.get(ev.pubkey, ev.kind, dtag))
      if (old) {
        if (old.created_at > ev.created_at || (old.created_at === ev.created_at && old.id <= ev.id)) return false
        delEv.run(old.id)
      }
    }
    const r = insEv.run(ev.id, ev.pubkey, ev.created_at, ev.kind, JSON.stringify(ev.tags), ev.content, ev.sig, nowSec(), expirationOf(ev), dtag)
    if (!r.changes) return false
    for (const t of ev.tags) if (t.length >= 2 && t[0].length === 1) insTag.run(ev.id, t[0], String(t[1]).slice(0, 256))
    return true
  }

  /** @param {any} ev @param {import('ws').WebSocket | null} source */
  function broadcast(ev, source) {
    for (const { ws, sid, filters } of subs.values()) {
      if (ws === source || ws.readyState !== ws.OPEN) continue
      if (filters.some(f => matches(ev, f))) { try { ws.send(JSON.stringify(['EVENT', sid, ev])) } catch {} }
    }
    for (const fn of listeners) { try { fn(ev) } catch {} }
  }

  /**
   * Ingresso unico per gli eventi (dai client e dai peer). Ritorna [accettato, messaggio].
   * @param {any} ev @param {import('ws').WebSocket | null} source @param {{ fromPeer?: boolean }} [meta]
   * @returns {[boolean, string]}
   */
  function ingest(ev, source, meta = {}) {
    if (!protocol.verifyEvent(ev, network)) return [false, 'invalid: evento non valido per questa rete']
    if (ev.created_at > nowSec() + MAX_FUTURE_SEC) return [false, 'invalid: created_at nel futuro']
    const exp = expirationOf(ev)
    if (exp !== null && exp <= nowSec()) return [false, 'invalid: evento scaduto']
    if (ctx.policy) {
      const [ok, why] = ctx.policy(ev, { fromPeer: !!meta.fromPeer })
      if (!ok) return [false, why]
    }
    const clean = { id: ev.id, pubkey: ev.pubkey, created_at: ev.created_at, kind: ev.kind, tags: ev.tags, content: ev.content, sig: ev.sig }
    if (isEphemeral(clean.kind)) { broadcast(clean, source); return [true, ''] }
    const fresh = store(clean)
    if (fresh) broadcast(clean, source)
    return [true, fresh ? '' : 'duplicate: già presente']
  }

  /** @param {Record<string, any>} f */
  function query(f) {
    const where = [], params = /** @type {any[]} */ ([])
    let joins = ''
    if (f.ids) { where.push('(' + f.ids.map(() => 'e.id LIKE ?').join(' OR ') + ')'); params.push(...f.ids.map((/** @type {string} */ x) => x + '%')) }
    if (f.authors) { where.push('(' + f.authors.map(() => 'e.pubkey LIKE ?').join(' OR ') + ')'); params.push(...f.authors.map((/** @type {string} */ x) => x + '%')) }
    if (f.kinds) { where.push(`e.kind IN (${f.kinds.map(() => '?').join(',')})`); params.push(...f.kinds) }
    if (f.since) { where.push('e.created_at >= ?'); params.push(f.since) }
    if (f.until) { where.push('e.created_at <= ?'); params.push(f.until) }
    where.push('(e.expires_at IS NULL OR e.expires_at > ?)'); params.push(nowSec())
    const joinParams = /** @type {any[]} */ ([])
    let n = 0
    for (const [k, vals] of Object.entries(f)) {
      if (!k.startsWith('#')) continue
      const a = `t${n++}`
      joins += ` JOIN event_tags ${a} ON ${a}.event_id = e.id AND ${a}.name = ? AND ${a}.value IN (${vals.map(() => '?').join(',')})`
      joinParams.push(k[1], ...vals)
    }
    const sql = `SELECT DISTINCT e.* FROM events e${joins}${where.length ? ' WHERE ' + where.join(' AND ') : ''} ORDER BY e.created_at DESC LIMIT ?`
    return db.prepare(sql).all(...joinParams, ...params, f.limit || 100).map((/** @type {any} */ r) => ({ id: r.id, pubkey: r.pubkey, created_at: r.created_at, kind: r.kind, tags: JSON.parse(r.tags), content: r.content, sig: r.sig }))
  }

  wss.on('connection', ws => {
    if (wss.clients.size > MAX_CONNECTIONS) { try { ws.close(1013, 'relay pieno') } catch {} return }
    const wsId = crypto.randomBytes(6).toString('hex')
    const mySubs = new Set()
    let tokens = EVENT_BURST, last = Date.now()
    const take = () => {
      const now = Date.now(), add = Math.floor((now - last) / EVENT_REFILL_MS)
      if (add > 0) { tokens = Math.min(EVENT_BURST, tokens + add); last = now }
      if (tokens <= 0) return false
      tokens--; return true
    }
    const send = (/** @type {any[]} */ m) => { try { if (ws.readyState === ws.OPEN) ws.send(JSON.stringify(m)) } catch {} }

    ws.on('message', raw => {
      try {
        let msg
        try { msg = JSON.parse(raw.toString()) } catch { return send(['NOTICE', 'invalid: non è JSON']) }
        if (!Array.isArray(msg) || typeof msg[0] !== 'string') return send(['NOTICE', 'invalid: atteso [tipo, ...]'])
        const [type, ...args] = msg
        if (type === 'EVENT') {
          const ev = args[0]
          if (!ev || typeof ev !== 'object' || typeof ev.id !== 'string') return send(['NOTICE', 'invalid: evento malformato'])
          if (!take()) return send(['OK', ev.id.slice(0, 64), false, 'rate-limited: rallenta'])
          const [ok, why] = ingest(ev, ws)
          return send(['OK', ev.id.slice(0, 64), ok, why])
        }
        if (type === 'REQ') {
          const [sid, ...raw] = args
          if (typeof sid !== 'string' || !sid || sid.length > 64) return send(['NOTICE', 'invalid: id sottoscrizione'])
          if (!raw.length || raw.length > MAX_FILTERS) return send(['CLOSED', sid, 'invalid: numero di filtri'])
          const filters = raw.map(sanitizeFilter)
          if (filters.some(f => !f)) return send(['CLOSED', sid, 'invalid: filtro malformato'])
          if (!mySubs.has(sid) && mySubs.size >= MAX_SUBS) return send(['CLOSED', sid, 'error: troppe sottoscrizioni'])
          subs.set(`${wsId}:${sid}`, { ws, sid, filters: /** @type {any} */ (filters) })
          mySubs.add(sid)
          for (const f of filters) for (const ev of query(/** @type {any} */ (f))) send(['EVENT', sid, ev])
          return send(['EOSE', sid])
        }
        if (type === 'CLOSE') {
          if (typeof args[0] !== 'string') return
          subs.delete(`${wsId}:${args[0]}`); mySubs.delete(args[0])
          return
        }
        send(['NOTICE', 'unsupported'])
      } catch (e) { log(`relay: errore nel messaggio (${/** @type {Error} */ (e).message})`) }
    })
    ws.on('error', () => {})
    ws.on('close', () => { for (const sid of mySubs) subs.delete(`${wsId}:${sid}`) })
  })

  /** Cancella gli eventi scaduti (NIP-40). Ritorna quanti. */
  function purgeExpired() { return Number(purgeStmt.run(nowSec()).changes) }
  const purgeTimer = setInterval(() => { try { purgeExpired() } catch {} }, 60000)
  purgeTimer.unref()

  return {
    wss, ingest, query, purgeExpired,
    /** @param {(ev: any) => void} fn */ onEvent: fn => { listeners.add(fn); return () => listeners.delete(fn) },
    close: () => { clearInterval(purgeTimer); for (const c of wss.clients) { try { c.terminate() } catch {} } wss.close() },
  }
}

module.exports = { createRelay, sanitizeFilter, MAX_PAYLOAD }
