// @ts-check
'use strict'
/**
 * I-02 — Il nodo è in ascolto solo su loopback (e sull'onion) se non configurato
 * diversamente; le funzioni dei ruoli diversi sono spente.
 */
const { test, after } = require('node:test')
const assert = require('node:assert/strict')
const net = require('net')
const os = require('os')
const { startTestNode, request } = require('../helpers/node')

/** @type {Awaited<ReturnType<typeof startTestNode>>} */ let t
after(() => t && t.close())

function canConnect(host, port) {
  return new Promise(resolve => {
    const s = net.connect({ host, port, timeout: 1500 }, () => { s.destroy(); resolve(true) })
    s.on('error', () => resolve(false))
    s.on('timeout', () => { s.destroy(); resolve(false) })
  })
}

test('configurazione vuota: raggiungibile su loopback, non dall\'IP di rete', async () => {
  t = await startTestNode()
  assert.equal((await request(t.node.port, '/health')).status, 200)
  const lan = Object.values(os.networkInterfaces()).flat().filter(n => n && n.family === 'IPv4' && !n.internal)
  for (const n of lan) {
    assert.equal(await canConnect(n.address, t.node.port), false, `in ascolto su ${n.address}`)
    if (t.node.ingressPort) assert.equal(await canConnect(n.address, t.node.ingressPort), false, 'ingresso onion esposto')
  }
})

test('ruolo user: le rotte dell\'hub non esistono', async () => {
  for (const p of ['/hub/upload', '/hub/status', '/hub/manifest/abc']) {
    assert.equal((await request(t.node.port, p, { headers: { Host: 'localhost' } })).status, 404, p)
  }
})

test('bind su interfacce esterne solo con conferma esplicita', async () => {
  const { startNode } = require('../../node')
  await assert.rejects(() => startNode({ dataDir: require('fs').mkdtempSync(require('path').join(os.tmpdir(), 'm4-b-')), network: 'm4tr1x-test', host: '0.0.0.0', port: 0, tor: false }), /esterne/)
})
