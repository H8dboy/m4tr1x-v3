// @ts-check
'use strict'
/**
 * I-39 — Le ancore non si lasciano riempire di schede false, e un attaccante non
 * può chiudere la porta ai nodi veri.
 * Dietro Tor tutti arrivano da 127.0.0.1: i limiti sono per chiave e globali.
 * - una chiave non può aggiornare la scheda più spesso del minimo
 * - prova di lavoro ADATTIVA sulla prima scheda di una chiave nuova: più chiavi
 *   nuove arrivano, più la prova è difficile. Non c'è un tetto fisso di chiavi
 *   nuove che un attaccante possa consumare: chi paga la prova entra sempre.
 * - tetto alle schede attive (protegge lo spazio, non blocca i rinnovi)
 * - l'ancora accetta solo schede di directory (non diventa un relay qualsiasi)
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const keys = require('../../packages/keys')
const protocol = require('../../packages/protocol')
const { requiredPowBits } = require('../../node/src/directory')
const { NET, startTestNode, wsOpen, wsExchange, testIdentity } = require('../helpers/node')

const onion = (/** @type {number} */ i) => String.fromCharCode(97 + (i % 26)).repeat(55) + 'a' + '.onion'
async function submit(port, ev) {
  const ws = await wsOpen(port)
  const r = await wsExchange(ws, ['EVENT', ev], m => m[0] === 'OK')
  ws.close()
  return r.at(-1)
}
const desc = (/** @type {any} */ k, /** @type {number} */ i, extra = {}) => protocol.buildDescriptor(k, NET, { onion: onion(i), roles: ['user'], version: '3', ttlSec: 3600, powBits: 0, ...extra })
/**
 * Scheda con MENO di `maxBits` bit di prova di lavoro. Una prova cercata per
 * N bit può averne di più per caso (1 volta su 4 ne ha N+2): per verificare un
 * rifiuto serve una scheda che sia davvero sotto la soglia.
 */
function descBelow(/** @type {any} */ k, /** @type {number} */ i, /** @type {number} */ maxBits) {
  const base = Math.floor(Date.now() / 1000)
  for (let j = 0; ; j++) {
    const d = desc(k, i, { createdAt: base - j })
    if (protocol.leadingZeroBits(d.id) < maxBits) return d
  }
}

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

test('difficoltà adattiva: sale di 2 bit a ogni raddoppio delle chiavi nuove nell\'ultima ora', () => {
  const L = { powBitsNewKey: 4, newKeysPerHourTarget: 10, powBitsMax: 30 }
  assert.equal(requiredPowBits(0, L), 4)
  assert.equal(requiredPowBits(9, L), 4)
  assert.equal(requiredPowBits(10, L), 6)
  assert.equal(requiredPowBits(20, L), 8)
  assert.equal(requiredPowBits(40, L), 10)
  assert.equal(requiredPowBits(1e9, L), 30, 'tetto alla difficoltà')
})

test('sotto pressione la prova si fa più difficile, ma chi la paga entra sempre (nessun tetto fisso)', async () => {
  const t = await startTestNode({ role: 'anchor', anchorLimits: { powBitsNewKey: 2, newKeysPerHourTarget: 3 } })
  try {
    for (let i = 0; i < 3; i++) assert.equal((await submit(t.node.port, desc(keys.generateNodeKey(), i, { powBits: 2 })))[2], true, `chiave ${i}`)
    // ora l'ancora è "sotto pressione": la prova minima non basta più…
    const cheap = await submit(t.node.port, descBelow(keys.generateNodeKey(), 10, 4))
    assert.equal(cheap[2], false)
    const need = Number(/servono almeno (\d+) bit/.exec(cheap[3])?.[1])
    assert.equal(need, 4, cheap[3])
    // …ma con la difficoltà richiesta si entra, anche dopo molte chiavi
    for (let i = 0; i < 8; i++) {
      const k = keys.generateNodeKey()
      let r = await submit(t.node.port, desc(k, 20 + i, { powBits: 2 }))
      if (!r[2]) r = await submit(t.node.port, desc(k, 20 + i, { powBits: Number(/(\d+) bit/.exec(r[3])?.[1]) }))
      assert.equal(r[2], true, `il nodo vero ${i} deve poter entrare pagando la prova: ${r[3]}`)
    }
  } finally { await t.close() }
})

test('il nodo legge la difficoltà richiesta e riprova da solo', async () => {
  const anchor = await startTestNode({ role: 'anchor', anchorLimits: { powBitsNewKey: 8 } })
  const n = await startTestNode({
    anchors: [`http://127.0.0.1:${anchor.node.port}`], allowLoopbackPeersForTests: true,
    descriptorOnionForTests: 'f'.repeat(56) + '.onion', descriptorPowBits: 1,
  })
  try {
    let found = null
    for (let i = 0; i < 60 && !found; i++) {
      await new Promise(r => setTimeout(r, 250))
      const ws = await wsOpen(anchor.node.port)
      const m = await wsExchange(ws, ['REQ', 'd', { kinds: [protocol.DESCRIPTOR_KIND] }], x => x[0] === 'EOSE')
      ws.close()
      found = m.find(x => x[0] === 'EVENT')
    }
    assert.ok(found, 'la scheda non è mai entrata')
    assert.ok(protocol.leadingZeroBits(found[2].id) >= 8)
  } finally { await n.close(); await anchor.close() }
})

test('tetto alle schede attive', async () => {
  const t = await startTestNode({ role: 'anchor', anchorLimits: { powBitsNewKey: 0, maxActive: 3 } })
  try {
    const res = []
    for (let i = 0; i < 5; i++) res.push((await submit(t.node.port, desc(keys.generateNodeKey(), i)))[2])
    assert.deepEqual(res, [true, true, true, false, false])
  } finally { await t.close() }
})

test('prova di lavoro richiesta per una chiave nuova', async () => {
  const t = await startTestNode({ role: 'anchor', anchorLimits: { powBitsNewKey: 12 } })
  try {
    const noPow = await submit(t.node.port, descBelow(keys.generateNodeKey(), 1, 12))
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

test("un'ancora-peer ha un budget di chiavi nuove all'ora: non può riempire le altre", async () => {
  const a = await startTestNode({ role: 'anchor', anchorLimits: { powBitsNewKey: 0, newKeysPerHourTarget: 1000 } })
  for (let i = 0; i < 8; i++) await submit(a.node.port, desc(keys.generateNodeKey(), i))
  const b = await startTestNode({ role: 'anchor', allowLoopbackPeersForTests: true, peers: [`http://127.0.0.1:${a.node.port}`],
    anchorLimits: { powBitsNewKey: 0, peerNewKeysPerHour: 3 } })
  try {
    await new Promise(r => setTimeout(r, 2500))
    const n = /** @type {any} */ (b.node.db.prepare('SELECT COUNT(*) AS n FROM events WHERE kind = ?').get(protocol.DESCRIPTOR_KIND)).n
    assert.equal(n, 3, `l'ancora B ha accettato ${n} chiavi nuove dalla stessa ancora-peer (budget 3)`)
  } finally { await b.close(); await a.close() }
})
