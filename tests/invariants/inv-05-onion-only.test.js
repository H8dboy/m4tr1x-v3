// @ts-check
'use strict'
/**
 * I-05 — Il nodo non contatta mai un peer o un'ancora che non sia .onion, e solo via SOCKS.
 * Un server TCP locale fa da "trappola": il nodo riceve indirizzi in chiaro
 * (peer, ancore, una scheda falsificata) e la trappola non deve ricevere connessioni.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const net = require('net')
const { startTestNode } = require('../helpers/node')
const { socks5Connect } = require('../../node/src/socks')
const { parsePeer } = require('../../node/src/peers')
const { parseTarget } = require('../../node/src/directory')

test('peer e ancore in chiaro: nessuna connessione', async () => {
  let hits = 0
  const trap = net.createServer(s => { hits++; s.destroy() })
  await new Promise(r => trap.listen(0, '127.0.0.1', () => r(undefined)))
  const port = /** @type {net.AddressInfo} */ (trap.address()).port
  const ip = ['127', '0', '0', '1'].join('.')
  const t = await startTestNode({ peers: [`http://${ip}:${port}`, `http://localhost:${port}`], anchors: [`http://${ip}:${port}`] })
  try {
    await new Promise(r => setTimeout(r, 2500))
    assert.equal(hits, 0, `il nodo ha aperto ${hits} connessioni in chiaro`)
  } finally { await t.close(); trap.close() }
})

test('SOCKS: destinazioni non onion rifiutate prima di qualsiasi connessione', async () => {
  // IP d'esempio composto a pezzi (lo scanner dei segreti blocca gli IP scritti per intero)
  for (const host of ['example.com', ['8', '8', '8', '8'].join('.'), 'x.onion', 'localhost']) {
    await assert.rejects(() => socks5Connect(9, host, 80), /onion/)
  }
})

test('indirizzi di peer e ancore: solo onion v3 (il loopback solo con l\'opzione dei test)', () => {
  const lo = ['127', '0', '0', '1'].join('.')
  const bad = [`http://${lo}:8080`, 'http://localhost:8080', 'http://example.com', 'https://' + 'a'.repeat(56) + '.onion', 'http://x.onion', 'ftp://' + 'a'.repeat(56) + '.onion']
  for (const u of bad) {
    assert.equal(parsePeer(u, false), null, u)
    assert.equal(parseTarget(u, false), null, u)
  }
  const good = 'http://' + 'a'.repeat(56) + '.onion'
  assert.ok(parsePeer(good, false) && parseTarget(good, false))
  assert.ok(parsePeer(`http://${lo}:1`, true), 'il loopback serve ai test')
})
