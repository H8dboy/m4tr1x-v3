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

/** @param {Partial<import('../../node/src/config').NodeConfig>} [over] */
async function startTestNode(over = {}) {
  const { startNode } = require('../../node')
  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'm4-node-'))
  const logs = /** @type {string[]} */ ([])
  const node = await startNode({ dataDir, network: NET, port: 0, tor: false, keyPassphrase: 'passphrase dei test, non segreta', log: line => logs.push(line), ...over })
  return { node, logs, dataDir, close: () => node.close() }
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

module.exports = { NET, startTestNode, request, wsOpen, wsExchange, testIdentity, uploadAuth }
