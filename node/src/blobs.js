// @ts-check
'use strict'
/**
 * Archivio dei file per hash (Blossom, ADR-0005).
 * - Il nome di un file è il suo SHA-256: chi lo riceve verifica (I-17).
 * - Caricamento autorizzato da un evento firmato kind 24242 (BUD-02) con
 *   t=upload, x=<sha256>, scadenza breve, e solo da pubkey in `uploaders`.
 * - Quota per pubkey e dimensione massima per file.
 */
const fs = require('fs')
const path = require('path')
const http = require('http')
const crypto = require('crypto')
const protocol = require('../../packages/protocol')
const { isOnionV3 } = require('./socks')

const SHA_RE = /^[0-9a-f]{64}$/
/** Tempo massimo totale di UN tentativo di scaricare un file da un peer (decisione del fondatore, 20/10/2026). */
const TEMPO_MASSIMO_BLOB_MS = 30000

/** @param {string} dataDir @param {string} sha */
function blobPath(dataDir, sha) {
  return path.join(dataDir, 'blobs', sha.slice(0, 2), sha)
}

/**
 * Verifica l'header Authorization di un upload. Ritorna la pubkey autorizzata o lancia.
 * @param {string|undefined} header @param {{ network: string, uploaders: string[] }} cfg
 * @returns {{ pubkey: string, x: string }}
 */
function checkUploadAuth(header, cfg) {
  if (typeof header !== 'string' || !header.startsWith('Nostr ')) throw Object.assign(new Error('autorizzazione mancante'), { status: 401 })
  let ev
  try { ev = JSON.parse(Buffer.from(header.slice(6), 'base64').toString('utf8')) } catch { throw Object.assign(new Error('autorizzazione illeggibile'), { status: 401 }) }
  if (!protocol.verifyEvent(ev, cfg.network) || ev.kind !== 24242) throw Object.assign(new Error('autorizzazione non valida'), { status: 401 })
  const tag = (/** @type {string} */ n) => ev.tags.find((/** @type {string[]} */ t) => t[0] === n)?.[1]
  const exp = Number(tag('expiration')), now = Math.floor(Date.now() / 1000)
  if (tag('t') !== 'upload' || !SHA_RE.test(tag('x') || '') || !(exp > now && exp <= now + 3600))
    throw Object.assign(new Error('autorizzazione scaduta o incompleta'), { status: 401 })
  if (!cfg.uploaders.includes(ev.pubkey)) throw Object.assign(new Error('questa chiave non può caricare su questo nodo'), { status: 403 })
  return { pubkey: ev.pubkey, x: /** @type {string} */ (tag('x')) }
}

/**
 * Riceve il corpo di un upload su file temporaneo calcolando l'hash; lo rende
 * definitivo solo se l'hash è quello autorizzato.
 * @param {import('http').IncomingMessage} req
 * @param {{ dataDir: string, maxBlobBytes: number, quotaBytes: number, db: import('node:sqlite').DatabaseSync }} ctx
 * @param {{ pubkey: string, x: string }} auth
 */
function receiveUpload(req, ctx, auth) {
  return new Promise((resolve, reject) => {
    const declared = Number(req.headers['content-length'])
    if (Number.isFinite(declared) && declared > ctx.maxBlobBytes) return reject(Object.assign(new Error('file troppo grande'), { status: 413 }))
    const used = /** @type {any} */ (ctx.db.prepare('SELECT COALESCE(SUM(size),0) AS s FROM blobs WHERE pubkey = ?').get(auth.pubkey)).s
    const tmpDir = path.join(ctx.dataDir, 'blobs', 'tmp')
    fs.mkdirSync(tmpDir, { recursive: true })
    const tmp = path.join(tmpDir, crypto.randomBytes(12).toString('hex'))
    const out = fs.createWriteStream(tmp, { mode: 0o600 })
    const h = crypto.createHash('sha256')
    let size = 0, failed = false
    const fail = (/** @type {Error & {status?: number}} */ e) => { if (failed) return; failed = true; req.unpipe(); out.destroy(); fs.rm(tmp, { force: true }, () => {}); reject(e) }
    req.on('data', c => {
      size += c.length
      if (size > ctx.maxBlobBytes) return fail(Object.assign(new Error('file troppo grande'), { status: 413 }))
      if (used + size > ctx.quotaBytes) return fail(Object.assign(new Error('quota esaurita'), { status: 507 }))
      h.update(c); out.write(c)
    })
    req.on('error', fail)
    req.on('end', () => {
      if (failed) return
      out.end(() => {
        const sha = h.digest('hex')
        if (sha !== auth.x) return fail(Object.assign(new Error('il contenuto non corrisponde all\'hash autorizzato'), { status: 400 }))
        const dest = blobPath(ctx.dataDir, sha)
        fs.mkdirSync(path.dirname(dest), { recursive: true })
        fs.renameSync(tmp, dest)
        ctx.db.prepare('INSERT OR IGNORE INTO blobs (sha256,size,pubkey,created_at) VALUES (?,?,?,?)').run(sha, size, auth.pubkey, Math.floor(Date.now() / 1000))
        resolve({ sha256: sha, size })
      })
    })
  })
}

/**
 * Salva un blob già verificato (replica da un peer).
 * @param {{ dataDir: string, db: import('node:sqlite').DatabaseSync }} ctx @param {string} sha @param {Buffer} data @param {string} pubkey
 */
function storeVerified(ctx, sha, data, pubkey) {
  if (crypto.createHash('sha256').update(data).digest('hex') !== sha) throw new Error('hash non corrispondente')
  const dest = blobPath(ctx.dataDir, sha)
  if (fs.existsSync(dest)) return
  fs.mkdirSync(path.dirname(dest), { recursive: true })
  const tmp = dest + '.' + crypto.randomBytes(6).toString('hex')
  fs.writeFileSync(tmp, data, { mode: 0o600 })
  fs.renameSync(tmp, dest)
  ctx.db.prepare('INSERT OR IGNORE INTO blobs (sha256,size,pubkey,created_at) VALUES (?,?,?,?)').run(sha, data.length, pubkey, Math.floor(Date.now() / 1000))
}

/**
 * Scarica un blob da un altro nodo e lo accetta solo se l'hash corrisponde (I-17).
 * Solo nodi .onion (via `agent` Tor); il loopback esiste solo per i test.
 * @param {string} baseUrl @param {string} sha
 * `onFase` riceve le fasi del trasferimento con i ms dall'inizio (connessione via Tor pronta,
 * risposta, fine): servono a capire DOVE si ferma un trasferimento appeso.
 * `tempoMassimoMs`: tempo massimo TOTALE del tentativo (connessione via Tor + risposta + corpo),
 * non solo di inattività: oltre, la richiesta si chiude con errore (decisione del fondatore, 20/10/2026: 30 s).
 * @param {{ agent?: import('http').Agent, allowLoopbackForTests?: boolean, headers?: Record<string,string>, maxBytes?: number, onFase?: (fase: string, ms: number) => void, tempoMassimoMs?: number }} [o]
 * @returns {Promise<Buffer>}
 */
function fetchVerified(baseUrl, sha, o = {}) {
  const t0 = Date.now()
  const fase = (/** @type {string} */ f) => o.onFase?.(f, Date.now() - t0)
  const tempoMassimo = o.tempoMassimoMs ?? TEMPO_MASSIMO_BLOB_MS
  /** @type {NodeJS.Timeout | undefined} */ let scadenza
  return new Promise((resolve, reject) => {
    if (!SHA_RE.test(sha)) return reject(new Error('hash non valido'))
    let u
    try { u = new URL(`/blob/${sha}`, baseUrl) } catch { return reject(new Error('URL del nodo non valido')) }
    const loop = o.allowLoopbackForTests && (u.hostname === '127.0.0.1' || u.hostname === 'localhost')
    if (u.protocol !== 'http:' || (!isOnionV3(u.hostname) && !loop)) return reject(new Error('solo nodi .onion'))
    if (!loop && !o.agent) return reject(new Error('serve l\'agente Tor'))
    const max = o.maxBytes || 64 * 1024 * 1024
    const req = http.get(u, { agent: loop ? undefined : o.agent, headers: o.headers, timeout: 120000 }, res => {
      fase(`risposta HTTP ${res.statusCode}`)
      if (res.statusCode !== 200) { res.resume(); return reject(new Error(`HTTP ${res.statusCode}`)) }
      const h = crypto.createHash('sha256'), parts = /** @type {Buffer[]} */ ([])
      let size = 0
      res.on('data', c => { size += c.length; if (size > max) { req.destroy(); return reject(new Error('blob troppo grande')) } h.update(c); parts.push(c) })
      res.on('end', () => {
        fase(`ricevuti ${size} byte`)
        if (h.digest('hex') !== sha) return reject(new Error('hash non corrispondente: blob rifiutato'))
        resolve(Buffer.concat(parts))
      })
      res.on('error', reject)
    })
    // con l'agente Tor il socket arriva solo dopo la stretta di mano SOCKS: qui il circuito verso l'onion è pronto
    req.on('socket', () => fase('connessione pronta'))
    req.on('timeout', () => req.destroy(new Error('tempo scaduto')))
    req.on('error', reject)
    // distrugge anche una connessione SOCKS ancora in corso: il socket che arriva dopo viene chiuso da http
    scadenza = setTimeout(() => req.destroy(new Error(`tempo massimo del tentativo superato (${tempoMassimo / 1000} s)`)), tempoMassimo)
  }).finally(() => clearTimeout(scadenza))
}

/**
 * Toglie un file dal nodo (cancellazione dell'autore, I-68): il file e la sua riga. Chi chiama ha già controllato
 * che sia dell'autore e che nessun altro contenuto rimasto lo usi.
 * @param {{ dataDir: string, db: import('node:sqlite').DatabaseSync }} ctx @param {string} sha
 */
function removeBlob(ctx, sha) {
  if (!SHA_RE.test(sha)) return
  try { fs.unlinkSync(blobPath(ctx.dataDir, sha)) } catch (e) { if (/** @type {any} */ (e).code !== 'ENOENT') throw e }
  ctx.db.prepare('DELETE FROM blobs WHERE sha256 = ?').run(sha)
}

module.exports = { SHA_RE, TEMPO_MASSIMO_BLOB_MS, blobPath, checkUploadAuth, receiveUpload, storeVerified, fetchVerified, removeBlob }
