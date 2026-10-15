// @ts-check
'use strict'
/**
 * I-38 — Le schede della directory scadono e si rinnovano: un nodo morto sparisce da solo.
 * - una scheda scaduta è rifiutata in verifica e in ingresso all'ancora
 * - l'ancora non la restituisce più e la cancella
 * - il rinnovo sostituisce la scheda precedente (una sola scheda attiva per nodo)
 * - il nodo rinnova da solo la propria scheda prima che scada
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const keys = require('../../packages/keys')
const protocol = require('../../packages/protocol')
const { NET, startTestNode, wsOpen, wsExchange } = require('../helpers/node')

const ONION = 'c'.repeat(56) + '.onion'
const now = () => Math.floor(Date.now() / 1000)
const anchorOpts = { role: /** @type {const} */ ('anchor'), anchorLimits: { powBitsNewKey: 0 } }

async function submit(port, ev) {
  const ws = await wsOpen(port)
  const r = await wsExchange(ws, ['EVENT', ev], m => m[0] === 'OK')
  ws.close()
  return r.at(-1)
}
async function list(port) {
  const ws = await wsOpen(port)
  const r = await wsExchange(ws, ['REQ', 'd', { kinds: [protocol.DESCRIPTOR_KIND] }], m => m[0] === 'EOSE')
  ws.close()
  return r.filter(m => m[0] === 'EVENT').map(m => m[2])
}

test('scheda scaduta: rifiutata in verifica', () => {
  const k = keys.generateNodeKey()
  const d = protocol.buildDescriptor(k, NET, { onion: ONION, roles: ['user'], version: '3', ttlSec: 60, powBits: 0 })
  assert.equal(protocol.verifyDescriptor(d, NET, now() + 120).ok, false)
})

test('ancora: scheda già scaduta rifiutata in ingresso', async () => {
  const t = await startTestNode(anchorOpts)
  try {
    const k = keys.generateNodeKey()
    const expired = protocol._signRawEventForTest({
      kind: protocol.DESCRIPTOR_KIND, created_at: now() - 7200, content: JSON.stringify({ roles: ['user'], version: '3' }),
      tags: [['d', 'm4tr1x-node'], ['onion', ONION], ['expiration', String(now() - 60)], ['net', NET]],
    }, k)
    const ok = await submit(t.node.port, expired)
    assert.equal(ok[2], false)
  } finally { await t.close() }
})

test('ancora: alla scadenza la scheda sparisce e viene cancellata', async () => {
  const t = await startTestNode(anchorOpts)
  try {
    const k = keys.generateNodeKey()
    const d = protocol.buildDescriptor(k, NET, { onion: ONION, roles: ['user'], version: '3', ttlSec: 2, powBits: 0 })
    assert.equal((await submit(t.node.port, d))[2], true)
    assert.equal((await list(t.node.port)).length, 1)
    await new Promise(r => setTimeout(r, 3500))
    assert.equal((await list(t.node.port)).length, 0, 'scheda scaduta ancora restituita')
    t.node.directory.purgeExpired()
    const left = /** @type {any} */ (t.node.db.prepare('SELECT COUNT(*) AS n FROM events WHERE kind = ?').get(protocol.DESCRIPTOR_KIND)).n
    assert.equal(left, 0, 'scheda scaduta ancora nel database')
  } finally { await t.close() }
})

test('ancora: il rinnovo sostituisce la scheda precedente', async () => {
  const t = await startTestNode({ ...anchorOpts, anchorLimits: { powBitsNewKey: 0, minUpdateIntervalSec: 0 } })
  try {
    const k = keys.generateNodeKey()
    const d1 = protocol.buildDescriptor(k, NET, { onion: ONION, roles: ['user'], version: '3.0.0', ttlSec: 3600, powBits: 0, createdAt: now() - 10 })
    const d2 = protocol.buildDescriptor(k, NET, { onion: ONION, roles: ['user'], version: '3.0.1', ttlSec: 3600, powBits: 0 })
    assert.equal((await submit(t.node.port, d1))[2], true)
    assert.equal((await submit(t.node.port, d2))[2], true)
    const l = await list(t.node.port)
    assert.equal(l.length, 1)
    assert.equal(l[0].id, d2.id)
  } finally { await t.close() }
})

test('il nodo rinnova da solo la propria scheda sulle ancore prima che scada', async () => {
  // intervallo minimo a 0 solo qui: il test rinnova ogni secondo (in produzione ~24 h)
  const anchor = await startTestNode({ ...anchorOpts, anchorLimits: { powBitsNewKey: 0, minUpdateIntervalSec: 0 } })
  const n = await startTestNode({
    anchors: [`http://127.0.0.1:${anchor.node.port}`], allowLoopbackPeersForTests: true,
    descriptorOnionForTests: ONION, descriptorTtlSec: 4, descriptorRenewSec: 1,
  })
  try {
    let first = null
    for (let i = 0; i < 40 && !first; i++) { await new Promise(r => setTimeout(r, 250)); first = (await list(anchor.node.port))[0] }
    assert.ok(first, 'scheda mai pubblicata')
    let renewed = null
    for (let i = 0; i < 40 && !renewed; i++) { await new Promise(r => setTimeout(r, 250)); const c = (await list(anchor.node.port))[0]; if (c && c.id !== first.id) renewed = c }
    assert.ok(renewed, 'scheda non rinnovata')
    assert.ok(Number(renewed.tags.find(t => t[0] === 'expiration')[1]) > Number(first.tags.find(t => t[0] === 'expiration')[1]))
  } finally { await n.close(); await anchor.close() }
})
