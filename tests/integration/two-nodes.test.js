// @ts-check
'use strict'
/**
 * Accettazione F2 — due nodi si scambiano eventi e blob.
 * (1) in locale (peer su loopback, solo in modalità test);
 * (2) via Tor vero: parte solo se M4TR1X_TOR_BIN punta a un binario `tor`
 *     (in CI lo installa il job "tor"); altrimenti viene saltata e lo dice.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const crypto = require('crypto')
const fs = require('fs')
const protocol = require('../../packages/protocol')
const { NET, startTestNode, request, wsOpen, wsExchange, testIdentity, uploadAuth } = require('../helpers/node')

async function waitFor(fn, ms = 20000, step = 250) {
  const end = Date.now() + ms
  while (Date.now() < end) { const v = await fn(); if (v) return v; await new Promise(r => setTimeout(r, step)) }
  return null
}

async function exchangeScenario(a, b, owner, peerPortForB) {
  // un file caricato su A e un evento su A che lo cita
  const data = crypto.randomBytes(20000)
  const auth = uploadAuth(owner, data)
  const up = await request(a.node.port, '/blob', { method: 'PUT', body: data, headers: { Authorization: auth.header } })
  assert.equal(up.status, 201, up.body.toString())
  const ev = protocol.buildEvent({ kind: 1, content: 'testimonianza di prova', tags: [['x', auth.sha]] }, owner, NET)
  const wsA = await wsOpen(a.node.port)
  const ok = await wsExchange(wsA, ['EVENT', ev], m => m[0] === 'OK')
  assert.equal(ok.at(-1)[2], true, JSON.stringify(ok.at(-1)))
  wsA.close()

  // B riceve l'evento dal peer…
  const gotEvent = await waitFor(async () => {
    const ws = await wsOpen(b.node.port)
    const msgs = await wsExchange(ws, ['REQ', 'q', { ids: [ev.id] }], m => m[0] === 'EOSE')
    ws.close()
    return msgs.find(m => m[0] === 'EVENT')
  }, 60000)
  assert.ok(gotEvent, 'B non ha ricevuto l\'evento da A')
  // …e il file citato, verificato per hash
  const gotBlob = await waitFor(async () => {
    const r = await request(b.node.port, '/blob/' + auth.sha)
    return r.status === 200 && r.body.equals(data)
  }, 60000)
  assert.ok(gotBlob, 'B non ha replicato il file da A')
}

test('due nodi in locale si scambiano evento e file', async () => {
  const owner = testIdentity()
  const a = await startTestNode({ uploaders: [owner.publicKey] })
  const b = await startTestNode({ peers: [`http://127.0.0.1:${a.node.port}`], allowLoopbackPeersForTests: true })
  try { await exchangeScenario(a, b, owner) }
  finally { await b.close(); await a.close() }
})

const TOR = process.env.M4TR1X_TOR_BIN
test('due nodi via Tor si scambiano evento e file', { skip: !TOR || !fs.existsSync(TOR) ? 'M4TR1X_TOR_BIN non impostato: prova via Tor saltata' : false, timeout: 600000 }, async () => {
  const owner = testIdentity()
  const a = await startTestNode({ uploaders: [owner.publicKey], tor: true, torBin: TOR })
  assert.ok(a.node.onion && /^[a-z2-7]{56}\.onion$/.test(a.node.onion), 'A non ha un onion')
  const b = await startTestNode({ peers: [`http://${a.node.onion}`], tor: true, torBin: TOR })
  try { await exchangeScenario(a, b, owner) }
  finally { await b.close(); await a.close() }
})
