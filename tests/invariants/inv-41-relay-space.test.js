// @ts-check
'use strict'
/**
 * I-41 — Lo spazio del relay è limitato: un autore o un peer non possono riempire il nodo.
 * - evento troppo grande: rifiutato
 * - quota per autore (numero di eventi): oltre, rifiutato
 * - tetto globale: oltre, rifiutati gli autori non fidati; gli autori fidati (il proprietario) passano
 * - la sincronizzazione dai peer rispetta gli stessi limiti
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const protocol = require('../../packages/protocol')
const { NET, startTestNode, wsOpen, wsExchange, testIdentity } = require('../helpers/node')

async function publish(port, events) {
  const ws = await wsOpen(port)
  const out = []
  for (const ev of events) out.push((await wsExchange(ws, ['EVENT', ev], m => m[0] === 'OK' && m[1] === ev.id)).at(-1))
  ws.close()
  return out
}
const post = (/** @type {any} */ who, /** @type {string} */ text, i = 0) =>
  protocol.buildEvent({ kind: 1, content: text, tags: [], created_at: Math.floor(Date.now() / 1000) - i }, who, NET)
const count = (/** @type {any} */ t) => /** @type {any} */ (t.node.db.prepare('SELECT COUNT(*) AS n FROM events').get()).n

test('evento troppo grande: rifiutato', async () => {
  const t = await startTestNode({ relayLimits: { maxEventBytes: 2000 } })
  try {
    const [r] = await publish(t.node.port, [post(testIdentity(), 'x'.repeat(5000))])
    assert.equal(r[2], false)
    assert.match(r[3], /troppo grande/)
  } finally { await t.close() }
})

test('quota per autore', async () => {
  const t = await startTestNode({ relayLimits: { perAuthorMaxEvents: 3 } })
  try {
    const spammer = testIdentity()
    const res = await publish(t.node.port, [0, 1, 2, 3, 4].map(i => post(spammer, `messaggio ${i}`, i)))
    assert.deepEqual(res.map(r => r[2]), [true, true, true, false, false])
    // un altro autore non è penalizzato dalla quota del primo
    const [other] = await publish(t.node.port, [post(testIdentity(), 'ciao')])
    assert.equal(other[2], true)
  } finally { await t.close() }
})

test('tetto globale: gli estranei si fermano, il proprietario passa', async () => {
  const owner = testIdentity()
  const t = await startTestNode({ uploaders: [owner.publicKey], relayLimits: { maxTotalEvents: 4 } })
  try {
    const strangers = await publish(t.node.port, [0, 1, 2, 3, 4, 5].map(i => post(testIdentity(), `estraneo ${i}`)))
    assert.equal(strangers.filter(r => r[2]).length, 4)
    assert.match(strangers.at(-1)[3], /pieno/)
    const [mine] = await publish(t.node.port, [post(owner, 'il proprietario scrive sempre')])
    assert.equal(mine[2], true, mine[3])
  } finally { await t.close() }
})

test('la sincronizzazione dai peer rispetta i limiti', async () => {
  const a = await startTestNode()
  const spammer = testIdentity()
  await publish(a.node.port, Array.from({ length: 12 }, (_, i) => post(spammer, `spam ${i}`, i)))
  const b = await startTestNode({ peers: [`http://127.0.0.1:${a.node.port}`], allowLoopbackPeersForTests: true, relayLimits: { perAuthorMaxEvents: 5 } })
  try {
    await new Promise(r => setTimeout(r, 2500))
    assert.ok(count(b) <= 5, `B ha accettato ${count(b)} eventi dello stesso autore oltre la quota`)
    assert.ok(count(b) > 0, 'la sincronizzazione deve comunque funzionare')
  } finally { await b.close(); await a.close() }
})
