// @ts-check
'use strict'
/** Supporto ai test del nodo: avvio in cartella temporanea, richieste HTTP e WebSocket. */
const fs = require('fs')
const os = require('os')
const path = require('path')
const http = require('http')
const crypto = require('crypto')
const WebSocket = require('ws')
const keys = require('../../packages/keys')
const protocol = require('../../packages/protocol')

const NET = 'm4tr1x-test'

/**
 * Cache del consenso di Tor tra i test (idea del fondatore, 16/10/2026).
 * All'avvio tor scarica il consenso (elenco firmato dei relay) e i microdescrittori:
 * da una cartella vuota è la parte lenta, a volte minuti. Si copiano SOLO i file
 * `cached-*` qui sotto: sono firmati dalle autorità di directory e tor li verifica
 * (firma e scadenza) quando li carica, quindi una cache vecchia o manomessa viene
 * scartata e riscaricata. MAI le chiavi (keys/, servizi onion) e MAI `state` (guardie):
 * ogni nodo di test sceglie le sue. Solo dentro la stessa macchina o lo stesso job di CI,
 * mai tra job (stesso motivo della cache npm spenta: avvelenamento).
 * M4TR1X_TOR_CACHE=0 la spegne (per misurare a freddo).
 */
const TOR_CACHE_DIR = path.join(os.tmpdir(), 'm4tr1x-tor-cache')
const TOR_CACHE_FILES = ['cached-certs', 'cached-microdesc-consensus', 'cached-microdescs', 'cached-microdescs.new', 'cached-consensus']
const torCacheOn = () => process.env.M4TR1X_TOR_CACHE !== '0'

/** Copia la cache nella cartella dati di tor. @returns {boolean} true se c'era qualcosa da copiare */
function seedTorCache(/** @type {string} */ torDir) {
  if (!torCacheOn()) return false
  fs.mkdirSync(torDir, { recursive: true, mode: 0o700 })
  let n = 0
  for (const f of TOR_CACHE_FILES) {
    const src = path.join(TOR_CACHE_DIR, f)
    if (fs.existsSync(src)) { fs.copyFileSync(src, path.join(torDir, f)); n++ }
  }
  return n > 0
}

/** Salva i file freschi di tor nella cache (scrittura atomica: copia + rinomina). */
function saveTorCache(/** @type {string} */ torDir) {
  if (!torCacheOn()) return
  fs.mkdirSync(TOR_CACHE_DIR, { recursive: true, mode: 0o700 })
  for (const f of TOR_CACHE_FILES) {
    const src = path.join(torDir, f)
    if (!fs.existsSync(src)) continue
    const tmp = path.join(TOR_CACHE_DIR, `${f}.${process.pid}.tmp`)
    try { fs.copyFileSync(src, tmp); fs.renameSync(tmp, path.join(TOR_CACHE_DIR, f)) } catch { try { fs.rmSync(tmp, { force: true }) } catch {} }
  }
}

/** @param {Partial<import('../../node/src/config').NodeConfig>} [over] */
async function startTestNode(over = {}) {
  const { startNode } = require('../../node')
  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'm4-node-'))
  const logs = /** @type {string[]} */ ([])
  // il nodo usa <dataDir>/tor come DataDirectory di tor (node/src/tor.js)
  const torDir = path.join(dataDir, 'tor')
  const torCacheWarm = over.tor ? seedTorCache(torDir) : false
  const node = await startNode({ dataDir, network: NET, port: 0, tor: false, keyPassphrase: 'passphrase dei test, non segreta', log: line => logs.push(line), ...over })
  return {
    node, logs, dataDir, torCacheWarm,
    close: async () => { await node.close(); if (over.tor) saveTorCache(torDir) },
  }
}

/**
 * @param {number} port @param {string} p
 * @param {{ method?: string, headers?: Record<string,string>, body?: Buffer|string }} [o]
 * @returns {Promise<{ status: number, headers: http.IncomingHttpHeaders, body: Buffer, json: any }>}
 */
function request(port, p, o = {}) {
  return new Promise((resolve, reject) => {
    const r = http.request({ host: '127.0.0.1', port, path: p, method: o.method || 'GET', headers: o.headers || {}, timeout: 15000 }, res => {
      const chunks = /** @type {Buffer[]} */ ([])
      res.on('data', c => chunks.push(c))
      res.on('end', () => {
        const body = Buffer.concat(chunks)
        let json = null; try { json = JSON.parse(body.toString('utf8')) } catch {}
        resolve({ status: res.statusCode || 0, headers: res.headers, body, json })
      })
    })
    r.on('error', reject)
    r.on('timeout', () => r.destroy(new Error('timeout')))
    if (o.body) r.write(o.body)
    r.end()
  })
}

/** @param {number} port @param {Record<string,string>} [headers] @returns {Promise<WebSocket>} */
function wsOpen(port, headers = {}) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`ws://127.0.0.1:${port}/relay`, { headers })
    ws.once('open', () => resolve(ws))
    ws.once('error', reject)
    ws.once('unexpected-response', (_req, res) => reject(new Error(`HTTP ${res.statusCode}`)))
  })
}

/** Invia un messaggio e raccoglie le risposte finché `until` non è vero. @param {WebSocket} ws @param {any[]} msg @param {(m:any[])=>boolean} until */
function wsExchange(ws, msg, until, timeoutMs = 10000) {
  return new Promise((resolve, reject) => {
    const got = /** @type {any[][]} */ ([])
    const t = setTimeout(() => { ws.off('message', on); reject(new Error('timeout: ' + JSON.stringify(got).slice(0, 200))) }, timeoutMs)
    const on = (/** @type {Buffer} */ d) => {
      let m; try { m = JSON.parse(d.toString()) } catch { return }
      got.push(m)
      if (until(m)) { clearTimeout(t); ws.off('message', on); resolve(got) }
    }
    ws.on('message', on)
    ws.send(JSON.stringify(msg))
  })
}

/** Identità di prova. */
function testIdentity() {
  return keys.deriveRoleKey(keys.seedFromMnemonic(keys.generateMnemonic()), 'identity')
}

/** Evento di autorizzazione Blossom (kind 24242) per caricare `data`. @param {any} identity @param {Buffer} data */
function uploadAuth(identity, data, opts = {}) {
  const sha = crypto.createHash('sha256').update(data).digest('hex')
  const ev = protocol.buildEvent({
    kind: 24242, content: 'upload',
    tags: [['t', 'upload'], ['x', opts.x || sha], ['expiration', String(Math.floor(Date.now() / 1000) + (opts.ttl ?? 300))]],
  }, identity, NET)
  return { sha, header: 'Nostr ' + Buffer.from(JSON.stringify(ev)).toString('base64') }
}

module.exports = { NET, TOR_CACHE_DIR, TOR_CACHE_FILES, startTestNode, request, wsOpen, wsExchange, testIdentity, uploadAuth }
