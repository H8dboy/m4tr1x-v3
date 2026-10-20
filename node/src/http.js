// @ts-check
'use strict'
/**
 * Ingresso HTTP del nodo. Due listener, stesso codice:
 *  - "owner"  127.0.0.1:<port>          → dal dispositivo del proprietario (admin possibile)
 *  - "public" 127.0.0.1:<ingressPort>   → dove punta il servizio onion (admin NON esiste)
 * Guardie, nell'ordine: Host ammesso (I-03) → CORS esatto (I-04) → rotte.
 * Nessun indirizzo dei client arriva mai al log (I-18).
 */
const fs = require('fs')
const crypto = require('crypto')
const blobs = require('./blobs')

const LOCAL_NAMES = new Set(['localhost', '127.0.0.1', '[::1]', '::1'])

/** Nome host senza porta, in minuscolo. @param {string|undefined} h */
function hostName(h) {
  const s = String(h || '').toLowerCase().trim()
  if (!s) return ''
  if (s.startsWith('[')) { const i = s.indexOf(']'); return i > 0 ? s.slice(0, i + 1) : '' }
  return s.split(':')[0]
}

/**
 * @param {object} ctx
 * @param {import('./config').NodeConfig} ctx.cfg
 * @param {import('node:sqlite').DatabaseSync} ctx.db
 * @param {(s: string) => void} ctx.log
 * @param {string} ctx.adminKey
 * @param {() => string | null} ctx.onion
 * @param {string} ctx.version
 */
function createHttp(ctx) {
  const { cfg, db, log } = ctx
  const adminBuf = Buffer.from(ctx.adminKey)

  /** @param {import('http').IncomingMessage} req @param {boolean} isPublic */
  function hostAllowed(req, isPublic) {
    const name = hostName(req.headers.host)
    if (!name) return false
    if (isPublic) return name === ctx.onion() || cfg.allowedHosts.includes(name)
    return LOCAL_NAMES.has(name) || cfg.allowedHosts.includes(name)
  }

  /** @param {import('http').IncomingMessage} req */
  function isAdmin(req) {
    const k = req.headers['x-admin-key']
    if (typeof k !== 'string') return false
    const b = Buffer.from(k)
    return b.length === adminBuf.length && crypto.timingSafeEqual(b, adminBuf)
  }

  /** @param {import('http').ServerResponse} res @param {number} status @param {any} body */
  function json(res, status, body) {
    const s = JSON.stringify(body)
    res.writeHead(status, { 'Content-Type': 'application/json; charset=utf-8', 'Content-Length': Buffer.byteLength(s) })
    res.end(s)
  }

  /**
   * @param {import('http').IncomingMessage} req @param {import('http').ServerResponse} res @param {boolean} isPublic
   */
  async function handle(req, res, isPublic) {
    res.setHeader('X-Content-Type-Options', 'nosniff')
    res.setHeader('Referrer-Policy', 'no-referrer')
    res.setHeader('X-Frame-Options', 'DENY')
    res.setHeader('Cache-Control', 'no-store')

    if (!hostAllowed(req, isPublic)) { req.resume(); return json(res, 421, { error: 'host non ammesso' }) }

    const origin = req.headers.origin
    if (typeof origin === 'string' && cfg.allowedOrigins.includes(origin)) {
      res.setHeader('Access-Control-Allow-Origin', origin)
      res.setHeader('Vary', 'Origin')
    }
    if (req.method === 'OPTIONS') {
      if (res.hasHeader('Access-Control-Allow-Origin')) {
        res.setHeader('Access-Control-Allow-Methods', 'GET, HEAD, PUT')
        res.setHeader('Access-Control-Allow-Headers', 'Authorization, Content-Type')
      }
      res.writeHead(204); return res.end()
    }

    let url
    try { url = new URL(req.url || '/', 'http://n') } catch { req.resume(); return json(res, 400, { error: 'richiesta non valida' }) }
    const p = url.pathname

    if (p === '/health' && req.method === 'GET') return json(res, 200, { ok: true, role: cfg.role, network: cfg.network, version: ctx.version })

    const blobMatch = /^\/blob\/([0-9a-f]{64})$/.exec(p)
    if (blobMatch && (req.method === 'GET' || req.method === 'HEAD')) {
      const file = blobs.blobPath(cfg.dataDir, blobMatch[1])
      let st
      try { st = fs.statSync(file) } catch { return json(res, 404, { error: 'non trovato' }) }
      res.writeHead(200, { 'Content-Type': 'application/octet-stream', 'Content-Length': st.size, 'Cache-Control': 'public, max-age=31536000, immutable' })
      if (req.method === 'HEAD') return res.end()
      // solo nei test (config.js): in produzione non si registra chi scarica cosa né quando
      const servito = cfg.onBlobServitoForTests
      if (servito) res.on('finish', () => servito(st.size))
      return fs.createReadStream(file).on('error', () => res.destroy()).pipe(res)
    }

    if (p === '/blob' && req.method === 'PUT') {
      try {
        const auth = blobs.checkUploadAuth(/** @type {string} */ (req.headers.authorization), cfg)
        const r = await blobs.receiveUpload(req, { dataDir: cfg.dataDir, maxBlobBytes: cfg.maxBlobBytes, quotaBytes: cfg.quotaBytes, db }, auth)
        log(`blob ricevuto (${r.size} byte)`)
        return json(res, 201, r)
      } catch (e) {
        req.resume()
        const err = /** @type {Error & {status?: number}} */ (e)
        return json(res, err.status || 500, { error: err.status ? err.message : 'errore interno' })
      }
    }

    // Admin e onion: esistono solo sul listener del proprietario (I-30, I-33)
    if (p === '/admin/status' || p === '/node/onion') {
      if (isPublic) return json(res, 404, { error: 'non trovato' })
      if (!isAdmin(req)) return json(res, 401, { error: 'chiave admin richiesta' })
      if (p === '/node/onion') return json(res, 200, { onion: ctx.onion() })
      const events = /** @type {any} */ (db.prepare('SELECT COUNT(*) AS n FROM events').get()).n
      const blobCount = /** @type {any} */ (db.prepare('SELECT COUNT(*) AS n FROM blobs').get()).n
      return json(res, 200, { role: cfg.role, network: cfg.network, events, blobs: blobCount, tor: !!ctx.onion() })
    }

    req.resume()
    return json(res, 404, { error: 'non trovato' })
  }

  return { handle, hostAllowed }
}

module.exports = { createHttp, hostName }
