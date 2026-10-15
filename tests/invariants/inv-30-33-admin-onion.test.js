// @ts-check
'use strict'
/**
 * I-30 — L'indirizzo onion del nodo utente è visibile solo al proprietario.
 * I-33 — L'admin è raggiungibile solo dal listener locale (non da quello
 *        dell'onion) e solo con la chiave admin K13, che non finisce nei log.
 */
const { test, before, after } = require('node:test')
const assert = require('node:assert/strict')
const { startTestNode, request } = require('../helpers/node')

/** @type {Awaited<ReturnType<typeof startTestNode>>} */ let t
const FAKE_ONION = 'a'.repeat(56) + '.onion'
before(async () => { t = await startTestNode({ allowedHosts: [FAKE_ONION] }) })
after(() => t.close())

const admin = () => ({ 'X-Admin-Key': t.node.adminKey })

test('I-30: /node/onion senza chiave admin → 401', async () => {
  assert.equal((await request(t.node.port, '/node/onion')).status, 401)
})

test('I-30: /node/onion con chiave admin → 200', async () => {
  assert.equal((await request(t.node.port, '/node/onion', { headers: admin() })).status, 200)
})

test('I-33: admin senza chiave o con chiave sbagliata → 401', async () => {
  assert.equal((await request(t.node.port, '/admin/status')).status, 401)
  assert.equal((await request(t.node.port, '/admin/status', { headers: { 'X-Admin-Key': 'x'.repeat(64) } })).status, 401)
})

test('I-33: admin con chiave dal listener locale → 200', async () => {
  assert.equal((await request(t.node.port, '/admin/status', { headers: admin() })).status, 200)
})

test('I-33: dal listener dell\'onion, admin e onion non esistono (404) anche con la chiave', async () => {
  assert.ok(t.node.ingressPort, 'listener di ingresso assente')
  for (const p of ['/admin/status', '/node/onion']) {
    assert.equal((await request(t.node.ingressPort, p, { headers: { ...admin(), Host: FAKE_ONION } })).status, 404, p)
  }
})

test("I-33: sul listener dell'onion un Host locale non è ammesso (421)", async () => {
  assert.equal((await request(t.node.ingressPort, '/admin/status', { headers: { ...admin(), Host: 'localhost' } })).status, 421)
})

test('I-33: la chiave admin non compare nei log', () => {
  assert.ok(t.node.adminKey && t.node.adminKey.length >= 64)
  for (const l of t.logs) assert.ok(!l.includes(t.node.adminKey), 'chiave admin nel log')
})
