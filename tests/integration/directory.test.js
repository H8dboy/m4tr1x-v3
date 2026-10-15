// @ts-check
'use strict'
/**
 * Accettazione F3 — directory replicata su 3 ancore.
 * - una scheda pubblicata su una sola ancora arriva alle altre due
 * - un'ancora spenta: la directory letta dalle altre resta completa
 * - un'ancora che nasconde un nodo viene compensata dalle altre (unione lato client)
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const keys = require('../../packages/keys')
const protocol = require('../../packages/protocol')
const directory = require('../../node/src/directory')
const { NET, startTestNode, wsOpen, wsExchange } = require('../helpers/node')

const onion = (/** @type {string} */ c) => c.repeat(56) + '.onion'
const fetchOpts = { network: NET, allowLoopbackForTests: true }

async function waitFor(fn, ms = 20000) {
  const end = Date.now() + ms
  while (Date.now() < end) { const v = await fn(); if (v) return v; await new Promise(r => setTimeout(r, 200)) }
  return null
}

test('3 ancore: replica, ancora spenta, ancora che nasconde', async () => {
  const lim = { powBitsNewKey: 0 }
  // ancore collegate tra loro su loopback: consentito solo in modalità test (in produzione solo onion, I-05)
  const opts = { role: /** @type {const} */ ('anchor'), anchorLimits: lim, allowLoopbackPeersForTests: true }
  const a1 = await startTestNode(opts)
  const a2 = await startTestNode(opts)
  const a3 = await startTestNode(opts)
  const url = (/** @type {any} */ a) => `http://127.0.0.1:${a.node.port}`
  // le ancore si collegano tra loro
  for (const [a, others] of [[a1, [a2, a3]], [a2, [a1, a3]], [a3, [a1, a2]]]) a.node.addPeers(others.map(url))
  try {
    const kA = keys.generateNodeKey(), kB = keys.generateNodeKey()
    const dA = protocol.buildDescriptor(kA, NET, { onion: onion('d'), roles: ['user'], version: '3', ttlSec: 3600, powBits: 0 })
    const dB = protocol.buildDescriptor(kB, NET, { onion: onion('e'), roles: ['edge'], version: '3', ttlSec: 3600, powBits: 0 })
    await directory.publishDescriptor([url(a1)], dA, fetchOpts)
    await directory.publishDescriptor([url(a2)], dB, fetchOpts)

    // replica: ogni ancora ha entrambe le schede
    for (const a of [a1, a2, a3]) {
      const ok = await waitFor(async () => (await directory.fetchDirectory([url(a)], fetchOpts)).length === 2)
      assert.ok(ok, 'replica incompleta')
    }

    // ancora che nasconde B (simulata cancellandolo dal suo database)
    a3.node.db.prepare('DELETE FROM events WHERE id = ?').run(dB.id)
    const only3 = await directory.fetchDirectory([url(a3)], fetchOpts)
    assert.equal(only3.length, 1)
    const merged = await directory.fetchDirectory([url(a1), url(a2), url(a3)], fetchOpts)
    assert.deepEqual(merged.map(d => d.onion).sort(), [onion('d'), onion('e')])

    // ancora spenta
    await a1.close()
    const without1 = await directory.fetchDirectory([url(a1), url(a2), url(a3)], fetchOpts)
    assert.deepEqual(without1.map(d => d.onion).sort(), [onion('d'), onion('e')])
  } finally { for (const a of [a2, a3, a1]) { try { await a.close() } catch {} } }
})
