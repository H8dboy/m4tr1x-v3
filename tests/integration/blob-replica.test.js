// @ts-check
'use strict'
/**
 * Scaricamento dei file da un peer (decisioni del fondatore, 20/10/2026):
 *   1) tempo massimo TOTALE di 30 s per tentativo (non solo di inattività);
 *   2) 3 tentativi con attesa crescente (2 s, 4 s), poi basta;
 *   3) "file servito" sul nodo che lo manda: solo nei test, mai in produzione
 *      (è una funzione: un file di configurazione JSON non può accenderlo).
 * Nato dal rosso del 20/10: fase "file" appesa a 60 s senza una riga di log.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const http = require('http')
const crypto = require('crypto')
const protocol = require('../../packages/protocol')
const blobs = require('../../node/src/blobs')
const { normalize } = require('../../node/src/config')
const { TENTATIVI_BLOB, ATTESA_BLOB_MS } = require('../../node/src/peers')
const { NET, startTestNode, request, wsOpen, wsExchange, testIdentity, uploadAuth } = require('../helpers/node')

async function waitFor(/** @type {() => any} */ fn, ms = 20000, step = 100) {
  const end = Date.now() + ms
  while (Date.now() < end) { const v = await fn(); if (v) return v; await new Promise(r => setTimeout(r, step)) }
  return null
}

/** A con l'evento che cita un file; il file si carica su A solo quando lo dice il test. */
async function scenario(/** @type {Record<string, any>} */ overA = {}) {
  const owner = testIdentity()
  const data = crypto.randomBytes(20000)
  const auth = uploadAuth(owner, data)
  const a = await startTestNode({ uploaders: [owner.publicKey], ...overA })
  const ev = protocol.buildEvent({ kind: 1, content: 'prova dei tentativi', tags: [['x', auth.sha]] }, owner, NET)
  const ws = await wsOpen(a.node.port)
  const ok = await wsExchange(ws, ['EVENT', ev], m => m[0] === 'OK')
  ws.close()
  assert.equal(ok.at(-1)[2], true, JSON.stringify(ok.at(-1)))
  const carica = async () => {
    const up = await request(a.node.port, '/blob', { method: 'PUT', body: data, headers: { Authorization: auth.header } })
    assert.equal(up.status, 201, up.body.toString())
  }
  const avviaB = () => startTestNode({ peers: [`http://127.0.0.1:${a.node.port}`], allowLoopbackPeersForTests: true })
  return { a, data, sha: auth.sha, carica, avviaB }
}

test('tempo massimo TOTALE per tentativo: un peer che manda un byte alla volta non tiene appeso il nodo', async () => {
  assert.equal(blobs.TEMPO_MASSIMO_BLOB_MS, 30000)
  // risponde subito e poi un byte ogni 200 ms: il timeout di inattività non scatterebbe mai
  /** @type {Set<NodeJS.Timeout>} */ const giri = new Set()
  const srv = http.createServer((_req, res) => {
    res.writeHead(200, { 'Content-Length': 1000000 })
    const g = setInterval(() => res.write('x'), 200); giri.add(g)
    res.on('close', () => clearInterval(g))
  })
  await new Promise(r => srv.listen(0, '127.0.0.1', () => r(undefined)))
  const port = /** @type {import('net').AddressInfo} */ (srv.address()).port
  const t0 = Date.now()
  try {
    // se il limite manca la richiesta resterebbe appesa: il test deve FALLIRE, non restare appeso anche lui
    /** @type {any} */ let guardia
    const appesa = new Promise((_, rej) => { guardia = setTimeout(() => rej(new Error('richiesta non chiusa entro 5 s: manca il tempo massimo totale')), 5000) })
    try {
      await assert.rejects(Promise.race([blobs.fetchVerified(`http://127.0.0.1:${port}`, 'a'.repeat(64), { allowLoopbackForTests: true, tempoMassimoMs: 1500 }), appesa]), /tempo massimo del tentativo/)
    } finally { clearTimeout(guardia) }
    const ms = Date.now() - t0
    assert.ok(ms >= 1400 && ms < 5000, `chiuso dopo ${ms} ms, atteso ~1500`)
  } finally { for (const g of giri) clearInterval(g); srv.closeAllConnections(); srv.close() }
})

test('3 tentativi con attesa crescente: il file che arriva tardi su A viene scaricato al tentativo dopo', async () => {
  assert.equal(TENTATIVI_BLOB, 3)
  const s = await scenario()
  const b = await s.avviaB()
  try {
    // il primo tentativo fallisce (il file non c'è ancora su A)…
    assert.ok(await waitFor(() => b.logs.some(l => l.includes('non replicato, tentativo 1/3'))), b.logs.join('\n'))
    await s.carica()
    // …e il secondo, dopo l'attesa, lo prende
    const preso = await waitFor(async () => { const r = await request(b.node.port, '/blob/' + s.sha); return r.status === 200 && r.body.equals(s.data) }, ATTESA_BLOB_MS * 4)
    assert.ok(preso, 'B non ha scaricato il file al tentativo successivo:\n' + b.logs.join('\n'))
    assert.ok(b.logs.some(l => l.includes('richiesto (tentativo 2/3)')), b.logs.join('\n'))
  } finally { await b.close(); await s.a.close() }
})

test('dopo 3 tentativi falliti si smette (nessun quarto tentativo)', async () => {
  const s = await scenario()
  const b = await s.avviaB()
  try {
    assert.ok(await waitFor(() => b.logs.some(l => l.includes('tentativo 3/3')), (ATTESA_BLOB_MS * 3 + 5000)), b.logs.join('\n'))
    // attese 2 s + 4 s: un quarto tentativo, se esistesse, partirebbe entro ~8 s
    await new Promise(r => setTimeout(r, ATTESA_BLOB_MS * 4 + 1000))
    const richieste = b.logs.filter(l => l.startsWith('peer: file richiesto'))
    assert.equal(richieste.length, 3, b.logs.join('\n'))
  } finally { await b.close(); await s.a.close() }
})

test('"file servito" solo nei test: una funzione passata dal codice; da un file di configurazione mai', async () => {
  // da JSON (come un file di configurazione di produzione) il valore si scarta, qualunque sia
  for (const v of [true, 'si', 1, { log: true }]) {
    const cfg = normalize(JSON.parse(JSON.stringify({ dataDir: '/tmp/x', network: NET, onBlobServitoForTests: v })))
    assert.equal(cfg.onBlobServitoForTests, undefined, `accettato da JSON: ${JSON.stringify(v)}`)
  }
  // nei test: A dice quanti byte ha servito
  /** @type {number[]} */ const serviti = []
  const s = await scenario({ onBlobServitoForTests: (/** @type {number} */ n) => serviti.push(n) })
  await s.carica()
  const b = await s.avviaB()
  try {
    assert.ok(await waitFor(() => serviti.length), 'A non ha segnalato il file servito')
    assert.deepEqual(serviti, [20000])
    // e nel log del nodo non finisce niente (in produzione non c'è proprio)
    assert.ok(!s.a.logs.some(l => /servit/i.test(l)), s.a.logs.join('\n'))
  } finally { await b.close(); await s.a.close() }
})

test("un file a pezzi (tag pezzo, ADR-0005) si replica pezzo per pezzo: l'hash intero non è un blob e non si chiede", async () => {
  const owner = testIdentity()
  const pezzi = [crypto.randomBytes(3000), crypto.randomBytes(2000)]
  const intero = Buffer.concat(pezzi)
  const shaIntero = crypto.createHash('sha256').update(intero).digest('hex')
  const auths = pezzi.map(p => uploadAuth(owner, p))
  const a = await startTestNode({ uploaders: [owner.publicKey] })
  const b = await startTestNode({ peers: [`http://127.0.0.1:${a.node.port}`], allowLoopbackPeersForTests: true })
  try {
    for (const [i, p] of pezzi.entries()) {
      const up = await request(a.node.port, '/blob', { method: 'PUT', body: p, headers: { Authorization: auths[i].header } })
      assert.equal(up.status, 201, up.body.toString())
    }
    // una scheda a celle: ogni pezzo con hash e misura; `x` e `size` sono del file intero, che come blob non esiste
    const tags = [['x', shaIntero], ['m', 'image/vnd.m4tr1x.celle'], ['size', String(intero.length)], ['t', 'm4tr1x-signal'], ...auths.map((x, i) => ['pezzo', x.sha, String(pezzi[i].length)])]
    const ev = protocol.buildEvent({ kind: 1063, content: 'foto a celle', tags }, owner, NET)
    const ws = await wsOpen(a.node.port)
    const ok = await wsExchange(ws, ['EVENT', ev], m => m[0] === 'OK')
    ws.close()
    assert.equal(ok.at(-1)[2], true, JSON.stringify(ok.at(-1)))
    for (const [i, x] of auths.entries()) {
      const preso = await waitFor(async () => { const r = await request(b.node.port, '/blob/' + x.sha); return r.status === 200 && r.body.equals(pezzi[i]) })
      assert.ok(preso, `B non ha replicato il pezzo ${i + 1}:\n` + b.logs.join('\n'))
    }
    const r = await request(b.node.port, '/blob/' + shaIntero)
    assert.equal(r.status, 404, 'il file intero non è un blob')
    assert.equal(b.logs.filter(l => l.startsWith('peer: file richiesto')).length, 2, "si chiedono i due pezzi, non l'intero:\n" + b.logs.join('\n'))
    assert.ok(!b.logs.some(l => l.includes('non replicato')), b.logs.join('\n'))
  } finally { await b.close(); await a.close() }
})
