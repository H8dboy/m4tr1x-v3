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

/** Scaricamento di un file da un peer: 3 tentativi, attese di 2 s e 4 s tra uno e l'altro (fondatore, 20/10/2026). */
const TENTATIVI_BLOB = 3
const ATTESA_BLOB_MS = 2000

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

  /** file in scaricamento: un evento rimandato dal peer durante i tentativi non ne apre altri in parallelo */
  /** @type {Set<string>} */ const inCorso = new Set()

  /** @param {string} peer @param {any} ev @param {{base: string, loop: boolean, hostHeader: string}} target */
  async function replicateBlobs(peer, ev, target) {
    for (const t of ev.tags) {
      if (t[0] !== 'x' || !blobs.SHA_RE.test(t[1] || '')) continue
      if (fs.existsSync(blobs.blobPath(cfg.dataDir, t[1])) || inCorso.has(t[1])) continue
      inCorso.add(t[1])
      try { await scaricaConTentativi(t[1], ev.pubkey, target) } finally { inCorso.delete(t[1]) }
    }
  }

  /**
   * Fino a TENTATIVI_BLOB tentativi da TEMPO_MASSIMO_BLOB_MS ciascuno, con attesa crescente tra uno e l'altro
   * (decisione del fondatore, 20/10/2026). Non si ritenta un file arrivato sbagliato (hash diverso o troppo
   * grande): il peer ha mandato quei dati, rimandarli non cambia niente.
   * Nel log: fasi con i tempi, mai indirizzi né hash (I-18, I-59).
   * @param {string} sha @param {string} pubkey @param {{base: string, loop: boolean, hostHeader: string}} target
   */
  async function scaricaConTentativi(sha, pubkey, target) {
    for (let n = 1; n <= TENTATIVI_BLOB && !stopped; n++) {
      const agent = agentFor(target.loop)
      if (agent === null) log(`peer: file, tentativo ${n}/${TENTATIVI_BLOB} non partito (Tor non pronto)`)
      else {
        try {
          log(`peer: file richiesto (tentativo ${n}/${TENTATIVI_BLOB})`)
          const data = await blobs.fetchVerified(target.base, sha, { agent, allowLoopbackForTests: target.loop, headers: { Host: target.hostHeader }, maxBytes: cfg.maxBlobBytes,
            onFase: (f, ms) => log(`peer: file, ${f} (${ms} ms)`) })
          if (stopped) return
          blobs.storeVerified({ dataDir: cfg.dataDir, db }, sha, data, pubkey)
          log('peer: file replicato e verificato')
          return
        } catch (e) {
          const msg = /** @type {Error} */ (e).message
          log(`peer: file non replicato, tentativo ${n}/${TENTATIVI_BLOB} (${msg})`)
          if (/hash non corrispondente|troppo grande/.test(msg)) return
        }
      }
      if (n < TENTATIVI_BLOB) await attendi(ATTESA_BLOB_MS * 2 ** (n - 1))
    }
  }

  /** attesa che si interrompe alla chiusura del nodo */
  function attendi(/** @type {number} */ ms) {
    return new Promise(r => {
      if (stopped) return r(undefined)
      const t = setTimeout(() => { timers.delete(t); r(undefined) }, ms)
      timers.add(t)
    })
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
      const [ok] = relay.ingest(ev, null, { fromPeer: true, peer })
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

module.exports = { startPeers, parsePeer, TENTATIVI_BLOB, ATTESA_BLOB_MS }
