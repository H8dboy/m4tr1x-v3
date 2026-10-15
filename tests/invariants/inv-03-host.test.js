// @ts-check
'use strict'
/** I-03 — Il nodo rifiuta richieste con Host non ammesso (DNS rebinding): 421 su ogni rotta e sul relay. */
const { test, before, after } = require('node:test')
const assert = require('node:assert/strict')
const { startTestNode, request, wsOpen } = require('../helpers/node')

/** @type {Awaited<ReturnType<typeof startTestNode>>} */ let t
before(async () => { t = await startTestNode() })
after(() => t.close())

for (const host of ['evil.example', 'evil.example:8080', 'localhost.evil.example', '127.0.0.1.nip.io', 'a'.repeat(56) + '.onion']) {
  test(`Host "${host.slice(0, 30)}" → 421`, async () => {
    for (const p of ['/health', '/blob/' + 'a'.repeat(64), '/admin/status', '/node/onion']) {
      assert.equal((await request(t.node.port, p, { headers: { Host: host } })).status, 421, p)
    }
  })
}

test('relay WebSocket con Host estraneo rifiutato', async () => {
  await assert.rejects(() => wsOpen(t.node.port, { Host: 'evil.example' }), /421|socket hang up|ECONNRESET/)
})

test('Host locale ammesso', async () => {
  for (const host of [`localhost:${t.node.port}`, `127.0.0.1:${t.node.port}`, `[::1]:${t.node.port}`, 'localhost']) {
    assert.equal((await request(t.node.port, '/health', { headers: { Host: host } })).status, 200, host)
  }
})
