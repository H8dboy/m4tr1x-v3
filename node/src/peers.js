// @ts-check
'use strict'
/**
 * Sincronizzazione con i peer configurati (F2: elenco statico; dalla F3 dalla directory).
 * - Solo peer .onion via Tor; il loopback esiste solo nei test.
 * - Ogni evento ricevuto passa da relay.ingest (stessa verifica dei client).
 * - I file citati negli eventi (tag "x") si scaricano dal peer e si accettano
 *   solo se l'hash corrisponde (I-17).
 */
const fs = require('fs')
const WebSocket = require('ws')
const blobs = require('./blobs')
const { isOnionV3, TorAgent } = require('./socks')

/**
 * @param {object} ctx
 * @param {import('./config').NodeConfig} ctx.cfg
 * @param {import('node:sqlite').DatabaseSync} ctx.db
 * @param {ReturnType<typeof import('./relay').createRelay>} ctx.relay
 * @param {(s: string) => void} ctx.log
 * @param {() => number | null} ctx.socksPort
 */
function startPeers(ctx) {
  const { cfg, db, relay, log } = ctx
  const getSince = db.prepare('SELECT since FROM peer_state WHERE peer = ?')
  const setSince = db.prepare('INSERT INTO peer_state (peer, since) VALUES (?, ?) ON CONFLICT(peer) DO UPDATE SET since = MAX(since, excluded.since)')
  let stopped = false
  /** @type {Set<WebSocket>} */ const sockets = new Set()
  /** @type {Set<NodeJS.Timeout>} */ const timers = new Set()

  const parse = (/** @type {string} */ peer) => parsePeer(peer, cfg.allowLoopbackPeersForTests)

  function agentFor(/** @type {boolean} */ loop) {
    if (loop) return undefined
    const p = ctx.socksPort()
    return p ? new TorAgent(p) : null
  }

  /** @param {string} peer @param {any} ev @param {{base: string, loop: boolean, hostHeader: string}} target */
  async function replicateBlobs(peer, ev, target) {
    for (const t of ev.tags) {
      if (t[0] !== 'x' || !blobs.SHA_RE.test(t[1] || '')) continue
      if (fs.existsSync(blobs.blobPath(cfg.dataDir, t[1]))) continue
      try {
        const agent = agentFor(target.loop)
        if (agent === null) return
        const data = await blobs.fetchVerified(target.base, t[1], { agent, allowLoopbackForTests: target.loop, headers: { Host: target.hostHeader }, maxBytes: cfg.maxBlobBytes })
        blobs.storeVerified({ dataDir: cfg.dataDir, db }, t[1], data, ev.pubkey)
        log('peer: file replicato e verificato')
      } catch (e) { log(`peer: file non replicato (${/** @type {Error} */ (e).message})`) }
    }
  }

  /** @param {string} peer @param {number} attempt */
  function connect(peer, attempt = 0) {
    if (stopped) return
    const target = parse(peer)
    if (!target) { log('peer ignorato: solo indirizzi .onion'); return }
    const agent = agentFor(target.loop)
    if (agent === null) { schedule(peer, attempt); return }
    const since = /** @type {any} */ (getSince.get(peer))?.since ?? 0
    const ws = new WebSocket(target.ws, { agent, headers: { Host: target.hostHeader }, handshakeTimeout: target.loop ? 10000 : 90000, maxPayload: 512 * 1024 })
    sockets.add(ws)
    ws.on('open', () => {
      log('peer: collegato')
      ws.send(JSON.stringify(['REQ', 'sync', { since: Math.max(0, since - 60), limit: 500 }]))
    })
    ws.on('message', raw => {
      let m; try { m = JSON.parse(raw.toString()) } catch { return }
      if (!Array.isArray(m) || m[0] !== 'EVENT' || m[1] !== 'sync') return
      const ev = m[2]
      const [ok] = relay.ingest(ev, null, { fromPeer: true })
      if (!ok) return
      setSince.run(peer, ev.created_at)
      replicateBlobs(peer, ev, target)
    })
    ws.on('error', () => {})
    ws.on('close', () => { sockets.delete(ws); schedule(peer, attempt + 1) })
  }

  function schedule(/** @type {string} */ peer, /** @type {number} */ attempt) {
    if (stopped) return
    const delay = Math.min(60000, 1000 * 2 ** Math.min(attempt, 6))
    const t = setTimeout(() => { timers.delete(t); connect(peer, attempt) }, delay)
    timers.add(t)
  }

  for (const p of cfg.peers) connect(p)
  return {
    /** Aggiunge peer a nodo avviato (dalla directory, o nei test). @param {string[]} list */
    add(list) { for (const p of list) if (!cfg.peers.includes(p)) { cfg.peers.push(p); connect(p) } },
    stop() {
      stopped = true
      for (const t of timers) clearTimeout(t)
      for (const ws of sockets) { try { ws.terminate() } catch {} }
    },
  }
}

/**
 * Indirizzo di un peer: solo http://<onion v3>; il loopback solo con l'opzione dei test.
 * @param {string} peer @param {boolean} allowLoop
 */
function parsePeer(peer, allowLoop) {
  let u
  try { u = new URL(peer) } catch { return null }
  const loop = allowLoop && (u.hostname === '127.0.0.1' || u.hostname === 'localhost')
  if (u.protocol !== 'http:' || (!isOnionV3(u.hostname) && !loop)) return null
  return { base: `http://${u.host}`, ws: `ws://${u.host}/relay`, loop, hostHeader: loop ? 'localhost' : u.hostname }
}

module.exports = { startPeers, parsePeer }
