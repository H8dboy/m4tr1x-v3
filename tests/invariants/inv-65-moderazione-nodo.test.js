// @ts-check
'use strict'
/**
 * I-65 nel nodo: le bandierine si calcolano all'arrivo dei contenuti PUBBLICI e si salvano per id di evento, mai per
 * autore; le segnalazioni NIP-56 (kind 1984) si contano per contenuto (persone diverse). Il proprietario le vede
 * solo dal listener locale con K13: senza chiave 401, dall'onion 404. Nella risposta: niente chi scrive, niente chi
 * segnala, niente testo. Regole di prova INNOCUE (l'elenco vero non sta nel repo).
 */
const { test, before, after } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const { DatabaseSync } = require('node:sqlite')
const protocol = require('../../packages/protocol')
const { startTestNode, request, wsOpen, wsExchange, testIdentity, NET } = require('../helpers/node')

const FAKE_ONION = 'c'.repeat(56) + '.onion'
const [anna, bruno, carla] = [testIdentity(), testIdentity(), testIdentity()]
const REGOLE = { versione: 7, regole: [{ id: 'r1', categoria: 'truffa', schema: 'soldi facili', peso: 2 }] }
/** @type {Awaited<ReturnType<typeof startTestNode>>} */ let t
/** @type {Record<string, any>} */ const ev = {}
const admin = () => ({ 'X-Admin-Key': t.node.adminKey })

before(async () => {
  t = await startTestNode({ regoleModerazione: REGOLE, allowedHosts: [FAKE_ONION] })
  const ws = await wsOpen(t.node.port)
  const manda = async (/** @type {any} */ e) => { const r = await wsExchange(ws, ['EVENT', e], m => m[0] === 'OK'); assert.equal(r.at(-1)[2], true, JSON.stringify(r.at(-1))) }
  ev.codice = protocol.buildEvent({ kind: 1, content: 'ecco s0ld1 f4c1l1 per te', tags: [] }, anna, NET)
  ev.pulito = protocol.buildEvent({ kind: 1, content: 'una bella giornata al mare', tags: [] }, anna, NET)
  ev.busta = protocol.buildEvent({ kind: 1059, content: 'soldi facili', tags: [['p', 'd'.repeat(64)]] }, anna, NET)
  // titoli: una discussione del forum con il titolo in codice, una scheda di reel (kind 1063) con il titolo in chiaro
  ev.discussione = protocol.buildEvent({ kind: 1, content: 'ciao a tutti', tags: [['t', 'm4tr1x-forum'], ['subject', 'S0LDI FACIL1']] }, anna, NET)
  ev.reel = protocol.buildEvent({ kind: 1063, content: 'il mio reel', tags: [['x', 'e'.repeat(64)], ['m', 'video/mp4'], ['size', '1000'], ['title', 'soldi facili']] }, anna, NET)
  for (const e of [ev.codice, ev.pulito, ev.busta, ev.discussione, ev.reel]) await manda(e)
  // due persone segnalano il post pulito (una due volte): contano 2
  for (const [chi, n] of [[bruno, 0], [bruno, 1], [carla, 0]])
    await manda(protocol.buildEvent({ kind: 1984, content: '', tags: [['e', ev.pulito.id, 'spam'], ['expiration', String(Math.floor(Date.now() / 1000) + 86400)]], created_at: Math.floor(Date.now() / 1000) - /** @type {number} */ (n) }, chi, NET))
  ws.close()
})
after(() => t.close())

test('I-65 nodo: bandierine solo sui contenuti pubblici, segnalazioni per persone diverse', async () => {
  const r = await request(t.node.port, '/admin/bandierine', { headers: admin() })
  assert.equal(r.status, 200, r.body.toString())
  assert.equal(r.json.versione_regole, 7)
  const per = Object.fromEntries(r.json.contenuti.map((/** @type {any} */ c) => [c.id, c]))
  assert.deepEqual(per[ev.codice.id].bandierine, [{ categoria: 'truffa', peso: 3, regole: ['r1'], codice: true }])
  assert.equal(per[ev.codice.id].segnalazioni, 0)
  assert.deepEqual(per[ev.pulito.id].bandierine, [])
  assert.equal(per[ev.pulito.id].segnalazioni, 2, 'segnalazioni non contate per persona')
  assert.ok(!per[ev.busta.id], 'una busta (messaggio privato) ha una bandierina')
  assert.deepEqual(per[ev.discussione.id]?.bandierine, [{ categoria: 'truffa', peso: 3, regole: ['r1'], codice: true }], 'titolo della discussione non analizzato')
  assert.deepEqual(per[ev.reel.id]?.bandierine, [{ categoria: 'truffa', peso: 2, regole: ['r1'], codice: false }], 'scheda del reel non analizzata')
})

test('I-65 nodo: nella risposta niente chi scrive, niente chi segnala, niente testo', async () => {
  const r = await request(t.node.port, '/admin/bandierine', { headers: admin() })
  const testo = r.body.toString('utf8')
  for (const s of [anna.publicKey, bruno.publicKey, carla.publicKey, 's0ld1', 'giornata', 'soldi'])
    assert.ok(!testo.includes(s), `nella risposta: ${s.slice(0, 12)}…`)
  for (const c of r.json.contenuti) assert.deepEqual(Object.keys(c).sort(), ['bandierine', 'creato', 'id', 'kind', 'segnalazioni'])
})

test('I-65 nodo: la tabella delle bandierine non ha colonne per persona, e il relay passa solo kind e testo', () => {
  const db = new DatabaseSync(path.join(t.dataDir, 'node.db'), { readOnly: true })
  const colonne = db.prepare('PRAGMA table_info(event_flags)').all().map((/** @type {any} */ c) => c.name).sort()
  db.close()
  assert.deepEqual(colonne, ['categoria', 'codice', 'event_id', 'peso', 'regole'])
  const relay = fs.readFileSync(path.join(__dirname, '..', '..', 'node', 'src', 'relay.js'), 'utf8')
  assert.match(relay, /ctx\.moderazione\(\{ kind: clean\.kind, content: testoPubblico\(clean\) \}\)/, 'il relay passa all\'analizzatore più di kind e testo')
  assert.match(relay, /\[e\.content, \.\.\.e\.tags\.filter\(t => \(t\[0\] === 'subject' \|\| t\[0\] === 'title'\) && typeof t\[1\] === 'string'\)\.map\(t => t\[1\]\)\]\.join/, 'il testo pubblico prende altro oltre a testo e titoli')
})

test('I-65 nodo: solo il proprietario con K13 (401 senza chiave, 404 dall\'onion)', async () => {
  assert.equal((await request(t.node.port, '/admin/bandierine')).status, 401)
  assert.equal((await request(t.node.ingressPort, '/admin/bandierine', { headers: { ...admin(), Host: FAKE_ONION } })).status, 404)
})

test('I-65 nodo: un elenco di regole storto ferma il nodo invece di moderare a metà', async () => {
  await assert.rejects(() => startTestNode({ regoleModerazione: { versione: 1, regole: [{ id: 'x', categoria: 'opinioni', schema: 'qualcosa', peso: 1 }] } }), /regole non valide/)
})

test('I-65 nodo: le app vedono le bandierine dei contenuti (dall\'onion), solo categoria, peso e codice', async () => {
  const ids = [ev.codice.id, ev.pulito.id, ev.busta.id].join(',')
  const r = await request(t.node.ingressPort, `/moderazione?ids=${ids}`, { headers: { Host: FAKE_ONION } })
  assert.equal(r.status, 200, r.body.toString())
  assert.deepEqual(r.json, { bandierine: { [ev.codice.id]: [{ categoria: 'truffa', peso: 3, codice: true }] } })
  const testo = r.body.toString('utf8')
  for (const s of ['r1', anna.publicKey, bruno.publicKey, 's0ld1', 'segnalazioni']) assert.ok(!testo.includes(s), `nella risposta pubblica: ${s.slice(0, 12)}`)
  // richieste storte: niente
  for (const q of ['', '?ids=', '?ids=../admin', `?ids=${Array(101).fill(ev.codice.id).join(',')}`])
    assert.equal((await request(t.node.ingressPort, `/moderazione${q}`, { headers: { Host: FAKE_ONION } })).status, 400, q.slice(0, 20))
})
