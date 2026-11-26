// @ts-check
'use strict'
/**
 * I-68 — Chi scrive può cancellare: un suo contenuto (NIP-09, kind 5) o il suo account intero (NIP-62, kind 62).
 *  (1) il nodo toglie SOLO contenuti dello stesso autore della cancellazione; una cancellazione di un altro non
 *      toglie niente e non blocca niente; la cancellazione resta come lapide e la copia vecchia non rientra;
 *  (2) una scheda cancellata porta via i suoi file, ma non quelli che un altro contenuto rimasto usa;
 *  (3) a quota piena si può ancora cancellare (solo se si toglie davvero qualcosa di proprio);
 *  (4) sparire toglie tutto dell'autore: contenuti, profilo, annuncio DM, le buste dei messaggi per le sue chiavi DM,
 *      i suoi file; resta UNA lapide; le copie vecchie non rientrano; gli altri non vengono toccati;
 *  (5) forme storte rifiutate;
 *  (6) nell'app: firma con `tag_cancella`/`tag_sparisci`; l'account si cancella solo con la parola CANCELLA e le
 *      chiavi lasciano il portachiavi solo DOPO il sì del nodo (prova dinamica in i31_ipc.rs: tor spento → chiavi
 *      intatte; "solo da questo dispositivo" → portachiavi vuoto); "Elimina" solo sui tuoi contenuti.
 */
const { test, before, after } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const vm = require('vm')
const crypto = require('crypto')
const { spawnSync } = require('child_process')
const protocol = require('../../packages/protocol')
const { startTestNode, request, wsOpen, wsExchange, testIdentity, uploadAuth, NET } = require('../helpers/node')

const ROOT = path.join(__dirname, '..', '..')
const [anna, bruno, carla, dario] = [testIdentity(), testIdentity(), testIdentity(), testIdentity()]
const ora = () => Math.floor(Date.now() / 1000)
/** @type {Awaited<ReturnType<typeof startTestNode>>} */ let t
const QUOTA = 6

before(async () => { t = await startTestNode({ uploaders: [anna.publicKey, carla.publicKey], relayLimits: { perAuthorMaxEvents: QUOTA } }) })
after(() => t.close())

/** @param {any} e */
async function manda(e) {
  const ws = await wsOpen(t.node.port)
  const r = await wsExchange(ws, ['EVENT', e], m => m[0] === 'OK')
  ws.close()
  return /** @type {[string, string, boolean, string]} */ (r.at(-1))
}
const c = (/** @type {any} */ chi, /** @type {number} */ kind, /** @type {string[][]} */ tags = [], content = '', created_at = ora()) => protocol.buildEvent({ kind, content, tags, created_at }, chi, NET)
const c_e = (/** @type {string} */ id) => t.node.db.prepare('SELECT COUNT(*) AS n FROM events WHERE id = ?').get(id).n === 1
/** carica un file come `chi` e ne restituisce l'hash @param {any} chi @param {Buffer} data */
async function carica(chi, data) {
  const a = uploadAuth(chi, data)
  const r = await request(t.node.port, '/blob', { method: 'PUT', body: data, headers: { Authorization: a.header } })
  assert.equal(r.status, 201, r.body.toString())
  return a.sha
}
const file = async (/** @type {string} */ sha) => (await request(t.node.port, '/blob/' + sha)).status
const scheda = (/** @type {any} */ chi, /** @type {string} */ sha, /** @type {number} */ n) => c(chi, 1063, [['x', sha], ['m', 'image/png'], ['size', String(n)], ['t', 'm4tr1x-signal']], 'scheda')

test('I-68 (1): si cancella solo il proprio; la lapide tiene fuori la copia vecchia', async () => {
  const post = c(anna, 1, [], 'da cancellare')
  assert.equal((await manda(post))[2], true)
  // bruno prova a cancellare il post di anna: accettato come evento, ma non toglie niente e non blocca anna
  assert.equal((await manda(c(bruno, 5, [['e', post.id]])))[2], true)
  assert.ok(c_e(post.id), 'la cancellazione di un altro ha tolto il post')
  const del = c(anna, 5, [['e', post.id], ['k', '1']])
  assert.equal((await manda(del))[2], true)
  assert.ok(!c_e(post.id), 'il post cancellato dal suo autore è ancora sul nodo')
  const r = await manda(post)
  assert.equal(r[2], false, 'la copia vecchia è rientrata dopo la cancellazione')
  assert.match(r[3], /^blocked: cancellato dal suo autore/)
  assert.ok(c_e(del.id), 'la lapide non è stata tenuta')
})

test('I-68 (2): una scheda cancellata porta via i suoi file, non quelli usati da altri', async () => {
  const solo = crypto.randomBytes(3000), comune = crypto.randomBytes(3000)
  const shaSolo = await carica(anna, solo), shaComune = await carica(anna, comune)
  const s1 = scheda(anna, shaSolo, solo.length), s2 = scheda(anna, shaComune, comune.length), s3 = scheda(carla, shaComune, comune.length)
  for (const s of [s1, s2, s3]) assert.equal((await manda(s))[2], true)
  assert.equal((await manda(c(anna, 5, [['e', s1.id], ['e', s2.id]])))[2], true)
  assert.equal(await file(shaSolo), 404, 'il file della scheda cancellata è ancora servito')
  assert.equal(fs.existsSync(path.join(t.dataDir, 'blobs', shaSolo.slice(0, 2), shaSolo)), false, 'il file è ancora sul disco')
  assert.equal(await file(shaComune), 200, 'tolto un file che la scheda di un altro usa ancora')
})

test('I-68 (3): a quota piena si cancella ancora, ma non si riempie il nodo di cancellazioni vuote', async () => {
  /** @type {any[]} */ const suoi = []
  for (let i = 0; i < QUOTA; i++) { const e = c(bruno, 1, [], `post ${i}`); if ((await manda(e))[2]) suoi.push(e) }
  assert.equal((await manda(c(bruno, 1, [], 'uno di troppo')))[3], 'blocked: quota di questo autore esaurita su questo nodo', 'la prova non ha riempito la quota')
  assert.equal((await manda(c(bruno, 5, [['e', 'f'.repeat(64)]])))[2], false, 'una cancellazione che non toglie niente salta la quota')
  assert.equal((await manda(c(bruno, 5, [['e', suoi[0].id]])))[2], true, 'a quota piena non si può cancellare')
  assert.ok(!c_e(suoi[0].id))
})

test('I-68 (4): sparire toglie tutto dell\'autore (anche le buste per le sue chiavi DM); una lapide sola', async () => {
  const dmAnna = 'd'.repeat(64)
  const prima = ora() - 10
  const post = c(anna, 1, [], 'post di anna', prima)
  const profilo = c(anna, 0, [], JSON.stringify({ name: 'Anna' }), prima)
  const annuncio = c(anna, 10477, [['dm', dmAnna, 'prova']], '', prima)
  const busta = c(dario, 1059, [['p', dmAnna]], 'cifrato', prima)
  const bustaAltro = c(dario, 1059, [['p', 'e'.repeat(64)]], 'cifrato per altri', prima)
  const diCarla = c(carla, 1, [], 'post di carla', prima)
  for (const e of [post, profilo, annuncio, busta, bustaAltro, diCarla]) assert.equal((await manda(e))[2], true, JSON.stringify(e.kind))
  const data = crypto.randomBytes(2000)
  const sha = await carica(anna, data)
  const via = c(anna, 62, [['relay', 'ALL_RELAYS']], '', prima + 5)
  assert.equal((await manda(via))[2], true)
  for (const [e, cosa] of [[post, 'post'], [profilo, 'profilo'], [annuncio, 'annuncio DM'], [busta, 'busta per la sua chiave DM']]) assert.ok(!c_e(e.id), `${cosa} ancora sul nodo`)
  assert.equal(await file(sha), 404, 'il suo file è ancora servito')
  for (const [e, cosa] of [[bustaAltro, 'busta per un altro'], [diCarla, 'post di un altro']]) assert.ok(c_e(e.id), `tolto anche ${cosa}`)
  const lapidi = t.node.db.prepare('SELECT kind FROM events WHERE pubkey = ? AND kind IN (5, 62)').all(anna.publicKey)
  assert.deepEqual(lapidi.map((/** @type {any} */ r) => r.kind), [62], 'resta più di una lapide (o nessuna)')
  const r = await manda(post)
  assert.equal(r[2], false, 'una copia vecchia è rientrata dopo la sparizione')
  assert.match(r[3], /^blocked: account cancellato/)
  // dopo la sparizione la stessa chiave può ricominciare (NIP-62): conta l'ora
  assert.equal((await manda(c(anna, 1, [], 'di nuovo qui', ora() + 2)))[2], true)
})

test('I-68 (5): forme storte rifiutate', async () => {
  for (const [kind, tags, perche] of [
    [5, [], 'cancellazione senza contenuti'], [5, [['e', 'XYZ']], 'id non valido'], [5, [['e', 'a'.repeat(64)], ['p', 'b'.repeat(64)]], 'tag p'],
    [5, Array.from({ length: 501 }, (_, i) => ['e', i.toString(16).padStart(64, '0')]), 'oltre 500 contenuti'],
    [62, [], 'sparire senza relay'], [62, [['relay', 'ALL_RELAYS'], ['p', 'a'.repeat(64)]], 'sparire con tag p'],
  ]) {
    const r = await manda(c(carla, /** @type {number} */ (kind), /** @type {string[][]} */ (tags)))
    assert.equal(r[2], false, `accettata: ${perche}`)
  }
})

const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })
test('I-68 (6): l\'app firma la forma giusta e toglie le chiavi solo dopo il sì del nodo', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 1800000 }, () => {
  const app = path.join(ROOT, 'app')
  for (const [crate, filtro, prova] of [['m4tr1x-protocol', 'prove_cancella', 'cancella_un_solo_contenuto_valido'], ['m4tr1x-identity', 'dimentica', 'dimentica_tutto_lascia_il_portachiavi_vuoto']]) {
    const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(app, 'Cargo.toml'), '-p', crate, '--lib', filtro],
      { cwd: app, encoding: 'utf8', timeout: 1800000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
    const out = (r.stdout || '') + (r.stderr || '')
    assert.equal(r.status, 0, out.slice(-4000))
    assert.match(out, new RegExp(`${prova} \\.\\.\\. ok`), `manca o fallisce la prova ${prova}`)
  }
  const lib = fs.readFileSync(path.join(app, 'shell', 'src', 'lib.rs'), 'utf8').replace(/\/\/[^\n]*/g, '')
  const acc = /async fn account_cancella\([\s\S]*?\n}\n/.exec(lib)
  assert.ok(acc, 'comando account_cancella non trovato')
  const corpo = acc[0]
  assert.ok(corpo.indexOf('conferma.trim() != PAROLA_CANCELLA') >= 0, 'l\'account si cancella senza la parola')
  const dopo = corpo.indexOf('esito?;'), via = corpo.indexOf('dimentica_tutto(')
  assert.ok(dopo > 0 && via > dopo, 'le chiavi lasciano il portachiavi prima del sì del nodo')
  assert.match(corpo, /firma_con_la_mia_identita\(&chiavi, m4tr1x_protocol::KIND_SPARISCI, "", &m4tr1x_protocol::tag_sparisci\(\)\)/)
  const can = /async fn cancella\([\s\S]*?\n}\n/.exec(lib)
  assert.ok(can && /m4tr1x_protocol::tag_cancella\(&id\)/.test(can[0]) && /KIND_CANCELLA, "", &tags/.test(can[0]), 'la cancellazione non usa tag_cancella')
  const i31 = fs.readFileSync(path.join(app, 'shell', 'tests', 'i31_ipc.rs'), 'utf8')
  assert.match(i31, /le chiavi sono sparite senza il sì del nodo/, 'manca la prova dinamica delle chiavi intatte senza il nodo')
})

// ─── Interfaccia ─────────────────────────────────────────────────────────────
class El {
  constructor(/** @type {string} */ tag) { this.tag = tag; this.children = /** @type {El[]} */ ([]); this.parent = /** @type {El|null} */ (null); this.hidden = false; this.disabled = false; this._t = ''; this.on = /** @type {Record<string, Function>} */ ({}); this.className = ''; this.classList = { add: () => {}, remove: () => {}, contains: () => false, toggle: () => {} }; this.attrs = /** @type {Record<string, string>} */ ({}) }
  get textContent() { return this._t + this.children.map(c => c.textContent).join('') }
  set textContent(v) { this._t = String(v); this.children = [] }
  set innerHTML(_v) { throw new Error('innerHTML usato') }
  append(/** @type {Array<El|string>} */ ...c) { for (const y of c) { const x = typeof y === 'string' ? Object.assign(new El('#testo'), { _t: y }) : y; x.parent = this; this.children.push(x) } }
  remove() { if (this.parent) this.parent.children = this.parent.children.filter(c => c !== this) }
  setAttribute(/** @type {string} */ k, /** @type {string} */ v) { this.attrs[k] = v }
  addEventListener(/** @type {string} */ e, /** @type {Function} */ f) { this.on[e] = f }
  trova(/** @type {(e: El) => boolean} */ f) { /** @type {El[]} */ const out = []; const giro = (/** @type {El} */ e) => { if (f(e)) out.push(e); e.children.forEach(giro) }; giro(this); return out }
}
const IO = 'a'.repeat(64), ALTRO = 'b'.repeat(64), ID = 'c'.repeat(64)
/** @type {Array<[string, any]>} */ const chiamate = []
const ctx = vm.createContext({
  window: { __TAURI__: { core: { invoke: async (/** @type {string} */ cmd, /** @type {any} */ args) => { chiamate.push([cmd, args]); return null } } } },
  document: { createElement: (/** @type {string} */ t) => new El(t), createElementNS: (/** @type {string} */ _n, /** @type {string} */ t) => new El(t), addEventListener: () => {}, getElementById: () => null, querySelectorAll: () => [] },
  Date, Math, Number, String, Array, Set, Object, JSON, setTimeout, clearTimeout, clearInterval, setInterval, console,
})
vm.runInContext(fs.readFileSync(path.join(ROOT, 'app', 'ui', 'main.js'), 'utf8'), ctx)
vm.runInContext(`identita = { identity: '${IO}' }`, ctx)
const elimina = (/** @type {El} */ li) => li.trova(e => e.className === 'tasto-azione' && e.textContent === 'Elimina')

test('I-68 (6): "Elimina" solo sui tuoi contenuti, al secondo tocco, e toglie il contenuto dopo il sì', async () => {
  const voce = (/** @type {string} */ autore) => /** @type {El} */ (ctx.voce({ id: ID, autore, creato: 1_760_000_000, testo: 'ciao', nome: null, bandierine: [] }))
  assert.equal(elimina(voce(ALTRO)).length, 0, '"Elimina" sul contenuto di un altro')
  const lista = new El('ol')
  const li = voce(IO)
  lista.append(li)
  const [b] = elimina(li)
  assert.ok(b, 'manca "Elimina" sul tuo contenuto')
  chiamate.length = 0
  await b.on.click({ stopPropagation() {} })
  assert.equal(chiamate.length, 0, 'cancellato al primo tocco')
  await b.on.click({ stopPropagation() {} })
  assert.equal(JSON.stringify(chiamate), JSON.stringify([['cancella', { id: ID }]]))
  assert.equal(lista.children.length, 0, 'il contenuto cancellato resta sullo schermo')
})
