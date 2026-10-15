// @ts-check
'use strict'
/**
 * Avvio del nodo M4TR1X v3 (ruolo user in F2; edge, hub, anchor dalle fasi successive).
 */
const fs = require('fs')
const path = require('path')
const http = require('http')
const crypto = require('crypto')
const { normalize } = require('./config')
const { makeLogger } = require('./log')
const { openDb } = require('./db')
const { createRelay } = require('./relay')
const { createHttp } = require('./http')
const { startPeers } = require('./peers')
const { startTor } = require('./tor')
const { assertPinnedNode } = require('./runtime')
const keys = require('../../packages/keys')
const directory = require('./directory')

const VERSION = '3.0.0-dev'

/** Chiave admin K13: generata una volta, file con permessi 600, mai nei log. @param {string} dataDir */
function loadAdminKey(dataDir) {
  const f = path.join(dataDir, 'admin.key')
  if (fs.existsSync(f)) return fs.readFileSync(f, 'utf8').trim()
  const k = crypto.randomBytes(32).toString('hex')
  fs.writeFileSync(f, k, { mode: 0o600 })
  return k
}

/** Chiave del nodo K5 (firma la scheda della directory). File con permessi 600. @param {string} dataDir */
function loadNodeKey(dataDir) {
  const f = path.join(dataDir, 'node.key')
  if (fs.existsSync(f)) return keys.roleKeyFromSecret(keys.NODE_ROLE, 1, Buffer.from(fs.readFileSync(f, 'utf8').trim(), 'hex'))
  const k = keys.generateNodeKey()
  fs.writeFileSync(f, Buffer.from(keys.exportSecret(k)).toString('hex'), { mode: 0o600 })
  return k
}

/** @param {http.Server} srv @param {string} host @param {number} port @returns {Promise<number>} */
function listen(srv, host, port) {
  return new Promise((resolve, reject) => {
    srv.once('error', reject)
    srv.listen(port, host, () => { srv.off('error', reject); resolve(/** @type {import('net').AddressInfo} */ (srv.address()).port) })
  })
}

/** @param {Partial<import('./config').NodeConfig>} input */
async function startNode(input) {
  const cfg = normalize(input)
  assertPinnedNode(cfg)
  const log = makeLogger(cfg.log)
  fs.mkdirSync(cfg.dataDir, { recursive: true, mode: 0o700 })
  const db = openDb(cfg.dataDir)
  const adminKey = cfg.adminKey || loadAdminKey(cfg.dataDir)
  let onion = /** @type {string|null} */ (null)
  let socksPort = /** @type {number|null} */ (null)

  const nodeKey = loadNodeKey(cfg.dataDir)
  const policy = cfg.role === 'anchor' ? directory.anchorPolicy({ db, network: cfg.network, limits: cfg.anchorLimits }) : undefined
  const relay = createRelay({ db, network: cfg.network, log, policy })
  const web = createHttp({ cfg, db, log, adminKey, onion: () => onion, version: VERSION })

  /** @param {boolean} isPublic */
  const makeServer = isPublic => {
    const srv = http.createServer({ requestTimeout: 120000, headersTimeout: 15000, maxHeaderSize: 16384 }, (req, res) => {
      web.handle(req, res, isPublic).catch(() => { try { res.writeHead(500); res.end() } catch {} })
    })
    srv.on('upgrade', (req, socket, head) => {
      socket.on('error', () => {})
      const p = (req.url || '').split('?')[0]
      if (!web.hostAllowed(req, isPublic)) { socket.end('HTTP/1.1 421 Misdirected Request\r\nConnection: close\r\n\r\n'); return }
      if (p !== '/relay') { socket.end('HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n'); return }
      relay.wss.handleUpgrade(req, socket, head, ws => relay.wss.emit('connection', ws, req))
    })
    srv.on('clientError', (_e, socket) => { try { socket.end('HTTP/1.1 400 Bad Request\r\n\r\n') } catch {} })
    return srv
  }

  const owner = makeServer(false)
  const pub = makeServer(true)
  const port = await listen(owner, cfg.host, cfg.port)
  const ingressPort = await listen(pub, '127.0.0.1', 0)
  log(`nodo avviato: ruolo ${cfg.role}, rete ${cfg.network}, porta ${port}${cfg.host === '127.0.0.1' || cfg.host === 'localhost' || cfg.host === '::1' ? ' (solo locale)' : ' (ESPOSTO)'}`)

  let tor = /** @type {Awaited<ReturnType<typeof startTor>> | null} */ (null)
  if (cfg.tor) {
    if (!cfg.torBin) throw new Error('tor: indicare torBin (binario tor verificato)')
    tor = await startTor({ bin: cfg.torBin, dataDir: cfg.dataDir, ingressPort, log })
    onion = tor.onion
    socksPort = tor.socksPort
  }

  const peers = startPeers({ cfg, db, relay, log, socksPort: () => socksPort })
  const publisher = directory.startPublisher({ cfg, nodeKey, onion: () => onion, socksPort: () => socksPort, log, version: VERSION })

  return {
    port, ingressPort, adminKey,
    get onion() { return onion },
    relay, db,
    nodePubkey: nodeKey.publicKey,
    directory: { purgeExpired: () => relay.purgeExpired(), publishNow: () => publisher.publishOnce() },
    /** @param {string[]} list */ addPeers: list => peers.add(list),
    async close() {
      publisher.stop()
      peers.stop()
      relay.close()
      await Promise.all([owner, pub].map(s => new Promise(r => { s.closeAllConnections?.(); s.close(() => r(undefined)) })))
      if (tor) tor.stop()
      try { db.close() } catch {}
      log('nodo fermato')
    },
  }
}

module.exports = { startNode, VERSION }
