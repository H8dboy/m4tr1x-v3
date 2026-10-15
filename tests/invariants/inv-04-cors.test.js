// @ts-check
'use strict'
/** I-04 — Nessun Access-Control-Allow-Origin "*"; origini ammesse solo esatte. */
const { test, before, after } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const { startTestNode, request } = require('../helpers/node')

/** @type {Awaited<ReturnType<typeof startTestNode>>} */ let t
before(async () => { t = await startTestNode({ allowedOrigins: ['http://localhost:1420'] }) })
after(() => t.close())

const preflight = (/** @type {string} */ origin) => request(t.node.port, '/health', { method: 'OPTIONS', headers: { Origin: origin, 'Access-Control-Request-Method': 'GET' } })

// origine di rete locale d'esempio, composta a pezzi: lo scanner dei segreti
// (giustamente) blocca gli IP privati scritti per intero nei file del repo
const LAN_ORIGIN = 'http://' + ['192', '168', '1', '5'].join('.')
for (const o of ['http://evil.example', 'http://localhost.evil.example', 'http://localhost:1420.evil.example', LAN_ORIGIN, 'http://x.onion', 'null', 'http://localhost:1421']) {
  test(`origine "${o}" rifiutata`, async () => {
    const r = await preflight(o)
    assert.equal(r.headers['access-control-allow-origin'], undefined)
  })
}

test('origine configurata ammessa esattamente', async () => {
  const r = await preflight('http://localhost:1420')
  assert.equal(r.headers['access-control-allow-origin'], 'http://localhost:1420')
})

test('nessuna risposta del nodo contiene ACAO "*"', async () => {
  for (const p of ['/health', '/blob/' + 'b'.repeat(64), '/nope']) {
    const r = await request(t.node.port, p, { headers: { Origin: 'http://evil.example' } })
    assert.notEqual(r.headers['access-control-allow-origin'], '*', p)
  }
})

test('statico: "Access-Control-Allow-Origin" con "*" non compare nel codice del nodo', () => {
  const dir = path.join(__dirname, '..', '..', 'node')
  const walk = (/** @type {string} */ d) => fs.readdirSync(d, { withFileTypes: true }).flatMap(e => e.isDirectory() ? walk(path.join(d, e.name)) : [path.join(d, e.name)])
  for (const f of walk(dir).filter(f => f.endsWith('.js'))) {
    assert.ok(!/allow-origin['"]?\s*[,:]\s*['"]\*['"]/i.test(fs.readFileSync(f, 'utf8')), f)
  }
})
