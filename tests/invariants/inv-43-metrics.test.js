// @ts-check
'use strict'
/**
 * I-43 — Le metriche del nodo non contengono IP, pubkey, onion o id di contenuti, e rispondono solo sul
 * listener del proprietario (loopback) con la chiave admin K13 (F2b, 24/11/2026).
 * Traffico di prova con valori riconoscibili (pubkey, id, hash del blob, testo, IP RFC 5737, onion finto):
 * nessuno compare nella risposta, che contiene SOLO numeri e booleani. Dal listener dell'onion: 404.
 */
const { test, before, after } = require('node:test')
const assert = require('node:assert/strict')
const crypto = require('crypto')
const fs = require('fs')
const path = require('path')
const protocol = require('../../packages/protocol')
const { startTestNode, request, wsOpen, wsExchange, testIdentity, uploadAuth, NET } = require('../helpers/node')
const { categoria } = require('../../node/src/metriche')

const owner = testIdentity()
const FAKE_ONION = 'b'.repeat(56) + '.onion'
const IP = '192.0.2.43'
const TESTO = 'SEGNO-RICONOSCIBILE-I43'
/** @type {Awaited<ReturnType<typeof startTestNode>>} */ let t
/** valori che NON devono comparire nelle metriche */ const segreti = /** @type {string[]} */ ([])
const admin = () => ({ 'X-Admin-Key': t.node.adminKey })

before(async () => {
  t = await startTestNode({ uploaders: [owner.publicKey], allowedHosts: [FAKE_ONION] })
  const ws = await wsOpen(t.node.port, { 'X-Forwarded-For': IP })
  const ok = (/** @type {any[]} */ m) => m[0] === 'OK'
  // evento valido, evento con firma falsa (rifiutato), evento ripetuto (duplicato)
  const buono = protocol.buildEvent({ kind: 1, content: TESTO, tags: [['t', TESTO.toLowerCase()]] }, owner, NET)
  await wsExchange(ws, ['EVENT', buono], ok)
  await wsExchange(ws, ['EVENT', buono], ok)
  const falso = { ...protocol.buildEvent({ kind: 1, content: TESTO + '-falso', tags: [] }, owner, NET), sig: 'f'.repeat(128) }
  await wsExchange(ws, ['EVENT', falso], ok)
  ws.close()
  // un blob caricato e uno rifiutato
  const data = Buffer.from(TESTO + crypto.randomBytes(32).toString('hex'))
  const a = uploadAuth(owner, data)
  assert.equal((await request(t.node.port, '/blob', { method: 'PUT', body: data, headers: { Authorization: a.header, 'X-Forwarded-For': IP } })).status, 201)
  const b = uploadAuth(owner, Buffer.from('altro'), { x: crypto.createHash('sha256').update('diverso').digest('hex') })
  await request(t.node.port, '/blob', { method: 'PUT', body: Buffer.from('altro'), headers: { Authorization: b.header } })
  // richieste dal listener dell'onion
  await request(t.node.ingressPort, '/health', { headers: { Host: FAKE_ONION, 'X-Forwarded-For': IP } })
  segreti.push(owner.publicKey, buono.id, falso.id, buono.sig, a.sha, b.sha, TESTO, TESTO.toLowerCase(), IP, FAKE_ONION, '.onion', '127.0.0.1', t.node.adminKey)
})
after(() => t.close())

/** @param {any} v @param {string} dove @returns {string[]} foglie che non sono numeri o booleani */
function foglieStrane(v, dove = '') {
  if (typeof v === 'number') return Number.isFinite(v) ? [] : [dove]
  if (typeof v === 'boolean') return []
  if (v && typeof v === 'object' && !Array.isArray(v)) return Object.entries(v).flatMap(([k, x]) => foglieStrane(x, `${dove}.${k}`))
  return [`${dove} (${typeof v})`]
}

test('I-43: metriche con la chiave admin dal listener locale, dopo il traffico di prova', async () => {
  const r = await request(t.node.port, '/admin/metrics', { headers: admin() })
  assert.equal(r.status, 200, r.body.toString())
  const m = r.json
  // contano davvero il traffico
  assert.equal(m.eventi.client.accettati, 1)
  assert.equal(m.eventi.client.duplicati, 1)
  assert.equal(m.eventi.client.rifiutati.invalid, 1)
  assert.equal(m.blob.ricevuti, 1)
  assert.equal(m.blob.rifiutati, 1)
  assert.ok(m.archivio.eventi >= 1 && m.archivio.blob === 1 && m.archivio.byte_blob > 0)
  assert.equal(m.tor, false)
})

test('I-43: nelle metriche solo numeri e booleani, nessun valore riconoscibile del traffico', async () => {
  const r = await request(t.node.port, '/admin/metrics', { headers: admin() })
  assert.deepEqual(foglieStrane(r.json), [], 'nelle metriche c\'è qualcosa che non è un contatore')
  const testo = r.body.toString('utf8').toLowerCase()
  for (const s of segreti) assert.ok(!testo.includes(s.toLowerCase()), `le metriche contengono un valore del traffico: ${s.slice(0, 16)}…`)
  // le chiavi stesse sono nomi fissi, non dati: nessuna stringa esadecimale lunga, nessun indirizzo
  assert.ok(!/[0-9a-f]{16,}/.test(testo) && !/\d+\.\d+\.\d+\.\d+/.test(testo), 'le metriche contengono un hash o un indirizzo')
})

test('I-43: senza chiave admin 401; dal listener dell\'onion non esistono (404) anche con la chiave', async () => {
  assert.equal((await request(t.node.port, '/admin/metrics')).status, 401)
  assert.equal((await request(t.node.port, '/admin/metrics', { headers: { 'X-Admin-Key': 'x'.repeat(64) } })).status, 401)
  assert.equal((await request(t.node.ingressPort, '/admin/metrics', { headers: { ...admin(), Host: FAKE_ONION } })).status, 404)
})

test('I-43: un motivo di rifiuto entra solo come categoria fissa', () => {
  assert.equal(categoria('invalid: evento ' + 'a'.repeat(64)), 'invalid')
  assert.equal(categoria('blocked: quota di ' + owner.publicKey), 'blocked')
  assert.equal(categoria(owner.publicKey + ': qualcosa'), 'altro')
  assert.equal(categoria(IP), 'altro')
})

test('I-43: il modulo delle metriche non importa niente e non tiene testi', () => {
  const src = fs.readFileSync(path.join(__dirname, '..', '..', 'node', 'src', 'metriche.js'), 'utf8').replace(/\/\/[^\n]*|\/\*[\s\S]*?\*\//g, '')
  assert.ok(!/require\(/.test(src), 'metriche.js importa moduli')
  assert.ok(!/\.push\(|new (Map|Set)\(/.test(src), 'metriche.js accumula elenchi: solo contatori')
})
