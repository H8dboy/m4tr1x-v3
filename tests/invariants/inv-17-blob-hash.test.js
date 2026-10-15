// @ts-check
'use strict'
/** I-17 — Un blob si accetta solo se il suo SHA-256 corrisponde: in upload, in lettura da altri nodi. */
const { test, before, after } = require('node:test')
const assert = require('node:assert/strict')
const http = require('http')
const crypto = require('crypto')
const { startTestNode, request, testIdentity, uploadAuth } = require('../helpers/node')
const blobs = require('../../node/src/blobs')

const owner = testIdentity()
/** @type {Awaited<ReturnType<typeof startTestNode>>} */ let t
before(async () => { t = await startTestNode({ uploaders: [owner.publicKey] }) })
after(() => t.close())

test('upload corretto, poi lettura identica', async () => {
  const data = crypto.randomBytes(5000)
  const a = uploadAuth(owner, data)
  const up = await request(t.node.port, '/blob', { method: 'PUT', body: data, headers: { Authorization: a.header } })
  assert.equal(up.status, 201, up.body.toString())
  assert.equal(up.json.sha256, a.sha)
  const get = await request(t.node.port, '/blob/' + a.sha)
  assert.equal(get.status, 200)
  assert.ok(get.body.equals(data))
})

test('upload il cui contenuto non corrisponde all\'hash autorizzato → rifiutato', async () => {
  const data = crypto.randomBytes(100)
  const a = uploadAuth(owner, data, { x: crypto.createHash('sha256').update('altro').digest('hex') })
  const up = await request(t.node.port, '/blob', { method: 'PUT', body: data, headers: { Authorization: a.header } })
  assert.equal(up.status, 400)
})

test('lettura da un altro nodo: byte alterati → rifiutati', async () => {
  const good = Buffer.from('contenuto vero')
  const sha = crypto.createHash('sha256').update(good).digest('hex')
  const evil = http.createServer((_q, s) => s.end('contenuto falso')).listen(0, '127.0.0.1')
  await new Promise(r => evil.once('listening', r))
  const port = /** @type {import('net').AddressInfo} */ (evil.address()).port
  try {
    await assert.rejects(() => blobs.fetchVerified(`http://127.0.0.1:${port}`, sha, { allowLoopbackForTests: true }), /hash/)
  } finally { evil.close() }
})

test('lettura da un altro nodo: byte corretti → accettati', async () => {
  const data = crypto.randomBytes(2000)
  const a = uploadAuth(owner, data)
  await request(t.node.port, '/blob', { method: 'PUT', body: data, headers: { Authorization: a.header } })
  const got = await blobs.fetchVerified(`http://127.0.0.1:${t.node.port}`, a.sha, { allowLoopbackForTests: true, headers: { Host: 'localhost' } })
  assert.ok(Buffer.from(got).equals(data))
})
