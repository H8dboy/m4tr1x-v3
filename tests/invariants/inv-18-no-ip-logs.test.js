// @ts-check
'use strict'
/**
 * I-18 — Il nodo non scrive indirizzi IP nei log, nemmeno 127.0.0.1: dietro Tor
 * tutti i client sono 127.0.0.1, ma un nodo esposto per errore loggherebbe IP veri.
 * Si eseguono avvio, richieste valide, errori e relay, poi si controllano i log.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const crypto = require('crypto')
const { startTestNode, request, wsOpen, wsExchange, testIdentity, uploadAuth } = require('../helpers/node')

const IPV4 = /\b(?:\d{1,3}\.){3}\d{1,3}\b/
const IPV6 = /\b(?:[0-9a-f]{1,4}:){2,7}[0-9a-f]{0,4}\b|::1\b|::ffff:/i

test('nessun IP nei log del nodo', async () => {
  const owner = testIdentity()
  const t = await startTestNode({ uploaders: [owner.publicKey] })
  try {
    await request(t.node.port, '/health')
    await request(t.node.port, '/nope')
    await request(t.node.port, '/health', { headers: { Host: 'evil.example' } })
    await request(t.node.port, '/admin/status')
    const data = crypto.randomBytes(300)
    await request(t.node.port, '/blob', { method: 'PUT', body: data, headers: { Authorization: uploadAuth(owner, data).header } })
    await request(t.node.port, '/blob', { method: 'PUT', body: data, headers: { Authorization: 'Nostr ' + Buffer.from('{}').toString('base64') } })
    const ws = await wsOpen(t.node.port)
    await wsExchange(ws, ['REQ', 's', { kinds: [1], limit: 1 }], m => m[0] === 'EOSE')
    ws.send('non json'); ws.close()
    await new Promise(r => setTimeout(r, 200))
  } finally { await t.close() }
  assert.ok(t.logs.length > 0, 'il nodo deve loggare qualcosa')
  for (const line of t.logs) {
    assert.ok(!IPV4.test(line) && !IPV6.test(line), `IP nel log: ${line}`)
  }
})
