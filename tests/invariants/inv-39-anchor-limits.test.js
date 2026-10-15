// @ts-check
'use strict'
/**
 * I-39 — Le ancore non si lasciano riempire di schede false.
 * Dietro Tor tutti arrivano da 127.0.0.1: i limiti sono per chiave e globali.
 * - una chiave non può aggiornare la scheda più spesso del minimo
 * - limite di chiavi nuove per ora (e per giorno)
 * - tetto alle schede attive
 * - prova di lavoro sulla prima scheda di una chiave nuova
 * - l'ancora accetta solo schede di directory (non diventa un relay qualsiasi)
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const keys = require('../../packages/keys')
const protocol = require('../../packages/protocol')
const { NET, startTestNode, wsOpen, wsExchange, testIdentity } = require('../helpers/node')

const onion = (/** @type {number} */ i) => String.fromCharCode(97 + (i % 26)).repeat(55) + 'a' + '.onion'
async function submit(port, ev) {
  const ws = await wsOpen(port)
  const r = await wsExchange(ws, ['EVENT', ev], m => m[0] === 'OK')
  ws.close()
  return r.at(-1)
}
const desc = (/** @type {any} */ k, /** @type {number} */ i, extra = {}) => protocol.buildDescriptor(k, NET, { onion: onion(i), roles: ['user'], version: '3', ttlSec: 3600, powBits: 0, ...extra })

test('aggiornamenti troppo frequenti della stessa chiave: rifiutati', async () => {
  const t = await startTestNode({ role: 'anchor', anchorLimits: { powBitsNewKey: 0, minUpdateIntervalSec: 600 } })
  try {
    const k = keys.generateNodeKey()
    assert.equal((await submit(t.node.port, desc(k, 1, { createdAt: Math.floor(Date.now() / 1000) - 5 })))[2], true)
    const second = await submit(t.node.port, desc(k, 1))
    assert.equal(second[2], false)
    assert.match(second[3], /rate-limited/)
  } finally { await t.close() }
})

test('troppe chiavi nuove in un\'ora: le eccedenti rifiutate', async () => {
  const t = await startTestNode({ role: 'anchor', anchorLimits: { powBitsNewKey: 0, maxNewKeysPerHour: 5, maxNewKeysPerDay: 100 } })
  try {
    let accepted = 0, limited = 0
    for (let i = 0; i < 9; i++) {
      const r = await submit(t.node.port, desc(keys.generateNodeKey(), i))
      if (r[2]) accepted++; else if (/rate-limited/.test(r[3])) limited++
    }
    assert.equal(accepted, 5)
    assert.equal(limited, 4)
  } finally { await t.close() }
})

test('tetto alle schede attive', async () => {
  const t = await startTestNode({ role: 'anchor', anchorLimits: { powBitsNewKey: 0, maxNewKeysPerHour: 100, maxActive: 3 } })
  try {
    const res = []
    for (let i = 0; i < 5; i++) res.push((await submit(t.node.port, desc(keys.generateNodeKey(), i)))[2])
    assert.deepEqual(res, [true, true, true, false, false])
  } finally { await t.close() }
})

test('prova di lavoro richiesta per una chiave nuova', async () => {
  const t = await startTestNode({ role: 'anchor', anchorLimits: { powBitsNewKey: 12 } })
  try {
    const noPow = await submit(t.node.port, desc(keys.generateNodeKey(), 1))
    assert.equal(noPow[2], false)
    assert.match(noPow[3], /pow/)
    const withPow = await submit(t.node.port, desc(keys.generateNodeKey(), 2, { powBits: 12 }))
    assert.equal(withPow[2], true, withPow[3])
  } finally { await t.close() }
})

test('l\'ancora rifiuta eventi che non sono schede di directory', async () => {
  const t = await startTestNode({ role: 'anchor', anchorLimits: { powBitsNewKey: 0 } })
  try {
    const post = protocol.buildEvent({ kind: 1, content: 'spam', tags: [] }, testIdentity(), NET)
    assert.equal((await submit(t.node.port, post))[2], false)
  } finally { await t.close() }
})
