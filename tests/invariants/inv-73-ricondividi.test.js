// @ts-check
'use strict'
/**
 * I-73 — Ricondividere non fa parlare nessuno a nome di altri. La ricondivisione (NIP-18, kind 6) porta SOLO l'id:
 * il testo mostrato viene dall'originale, chiesto al nodo e verificato come ogni post; una copia incollata nel testo
 * non si usa mai.
 *  (1) nodo: si pubblica, si rilegge, si toglie con una cancellazione (I-68);
 *  (2) Rust: ricondivisione e originale verificati, una per persona e post, niente auto-ricondivisione; firma con
 *      `tag_ricondividi` e testo vuoto; chi hai bloccato non compare nemmeno come chi ricondivide;
 *  (3) interfaccia: "Ricondividi" solo su post di altri, con identità, al secondo tocco; "Togli" solo sulla TUA.
 */
const { test, before, after } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const vm = require('vm')
const { spawnSync } = require('child_process')
const protocol = require('../../packages/protocol')
const { startTestNode, wsOpen, wsExchange, testIdentity, NET } = require('../helpers/node')

const ROOT = path.join(__dirname, '..', '..')
const APP = path.join(ROOT, 'app')
const [anna, bruno] = [testIdentity(), testIdentity()]
/** @type {Awaited<ReturnType<typeof startTestNode>>} */ let t
before(async () => { t = await startTestNode() })
after(() => t.close())

/** @param {any} msg @param {(m: any) => boolean} fine */
async function scambia(msg, fine) {
  const ws = await wsOpen(t.node.port)
  const r = await wsExchange(ws, msg, fine)
  ws.close()
  return r
}
const manda = async (/** @type {any} */ e) => /** @type {any} */ ((await scambia(['EVENT', e], m => m[0] === 'OK')).at(-1))[2]
const c = (/** @type {any} */ chi, /** @type {number} */ kind, /** @type {string[][]} */ tags = [], content = '') => protocol.buildEvent({ kind, content, tags, created_at: Math.floor(Date.now() / 1000) }, chi, NET)

test('I-73 (1): la ricondivisione si pubblica, si rilegge e si toglie', async () => {
  const post = c(anna, 1, [], 'originale')
  assert.equal(await manda(post), true)
  const r = c(bruno, 6, [['e', post.id], ['k', '1']])
  assert.equal(await manda(r), true)
  const letti = (await scambia(['REQ', 's', { kinds: [1, 6], limit: 50 }], m => m[0] === 'EOSE')).filter(m => m[0] === 'EVENT').map(m => m[2].id)
  assert.ok(letti.includes(r.id) && letti.includes(post.id))
  assert.equal(await manda(c(bruno, 5, [['e', r.id], ['k', '6']])), true)
  const dopo = (await scambia(['REQ', 's', { kinds: [6], limit: 50 }], m => m[0] === 'EOSE')).filter(m => m[0] === 'EVENT')
  assert.equal(dopo.length, 0, 'la ricondivisione tolta si rilegge ancora')
})

const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })
test('I-73 (2): verificate entrambe, firma con tag_ricondividi, bloccati fuori', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 1800000 }, () => {
  for (const [args, prova] of [[['-p', 'm4tr1x-feed', '--test', 'ricondividi'], 'solo_ricondivisioni_verificate_di_originali_verificati'], [['-p', 'm4tr1x-protocol', '--lib', 'prove_ricondividi'], 'ricondivisione_solo_nella_forma_giusta']]) {
    const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(APP, 'Cargo.toml'), ...args],
      { cwd: APP, encoding: 'utf8', timeout: 1800000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
    const out = (r.stdout || '') + (r.stderr || '')
    assert.equal(r.status, 0, out.slice(-4000))
    assert.match(out, new RegExp(`${prova} \\.\\.\\. ok`))
  }
  const lib = fs.readFileSync(path.join(APP, 'shell', 'src', 'lib.rs'), 'utf8').replace(/\/\/[^\n]*/g, '')
  const cmd = /async fn ricondividi\([\s\S]*?\n}\n/.exec(lib)
  assert.ok(cmd, 'comando ricondividi non trovato')
  assert.match(cmd[0], /firma_con_la_mia_identita\(&chiavi, m4tr1x_protocol::KIND_RICONDIVISIONE, "", &tags\)/, 'la ricondivisione porta un testo')
  assert.match(cmd[0], /m4tr1x_protocol::tag_ricondividi\(&id\)/)
  const feed = /async fn feed\([\s\S]*?\n}\n/.exec(lib)
  assert.ok(feed && /p\.ricondivisione\.as_ref\(\)\.is_none_or\(\|r\| !bloccati\.contiene\(&r\.da\)\)/.test(feed[0]), 'le ricondivisioni di chi hai bloccato compaiono')
  const crate = fs.readFileSync(path.join(APP, 'feed', 'src', 'lib.rs'), 'utf8')
  const ric = /pub fn ricondivisioni\([\s\S]*?\n}\n/.exec(crate)
  assert.ok(ric && !/content/.test(ric[0].replace(/\/\/[^\n]*/g, '')), 'il testo della ricondivisione viene usato')
})

// ─── Interfaccia ─────────────────────────────────────────────────────────────
class El {
  constructor(/** @type {string} */ tag) { this.tag = tag; this.children = /** @type {El[]} */ ([]); this.parent = /** @type {El|null} */ (null); this.hidden = false; this.disabled = false; this.title = ''; this._t = ''; this.on = /** @type {Record<string, Function>} */ ({}); this.className = ''; this.classList = { add: () => {}, remove: () => {}, contains: () => false, toggle: () => {} }; this.attrs = /** @type {Record<string, string>} */ ({}) }
  get textContent() { return this._t + this.children.map(c => c.textContent).join('') }
  set textContent(v) { this._t = String(v); this.children = [] }
  set innerHTML(_v) { throw new Error('innerHTML usato') }
  append(/** @type {Array<El|string>} */ ...c) { for (const y of c) { const x = typeof y === 'string' ? Object.assign(new El('#testo'), { _t: y }) : y; x.parent = this; this.children.push(x) } }
  remove() { if (this.parent) this.parent.children = this.parent.children.filter(c => c !== this) }
  setAttribute(/** @type {string} */ k, /** @type {string} */ v) { this.attrs[k] = v }
  addEventListener(/** @type {string} */ e, /** @type {Function} */ f) { this.on[e] = f }
  trova(/** @type {(e: El) => boolean} */ f) { /** @type {El[]} */ const out = []; const giro = (/** @type {El} */ e) => { if (f(e)) out.push(e); e.children.forEach(giro) }; giro(this); return out }
}
const IO = 'a'.repeat(64), ALTRO = 'b'.repeat(64), ID = 'c'.repeat(64), RID = 'd'.repeat(64)
/** @type {Array<[string, any]>} */ const chiamate = []
const ctx = vm.createContext({
  window: { __TAURI__: { core: { invoke: async (/** @type {string} */ cmd, /** @type {any} */ args) => { chiamate.push([cmd, args]); return null } } } },
  document: { createElement: (/** @type {string} */ t) => new El(t), createElementNS: (/** @type {string} */ _n, /** @type {string} */ t) => new El(t), addEventListener: () => {}, getElementById: () => null, querySelectorAll: () => [] },
  Date, Math, Number, String, Array, Set, Object, JSON, setTimeout, clearTimeout, clearInterval, setInterval, console,
})
vm.runInContext(fs.readFileSync(path.join(ROOT, 'app', 'ui', 'main.js'), 'utf8'), ctx)
vm.runInContext(`identita = { identity: '${IO}' }`, ctx)
const voce = (/** @type {any} */ extra) => /** @type {El} */ (ctx.voce({ id: ID, autore: ALTRO, creato: 1_760_000_000, testo: 'ciao', nome: null, bandierine: [], ...extra }))
const tasti = (/** @type {El} */ li, /** @type {string} */ testo) => li.trova(e => e.className.startsWith('tasto-azione') && e.textContent === testo)

test('I-73 (3): "Ricondividi" solo su post di altri, al secondo tocco', async () => {
  assert.equal(tasti(voce({ autore: IO }), 'Ricondividi').length, 0, '"Ricondividi" sul tuo post')
  assert.equal(tasti(voce({ ricondiviso_da: ALTRO, ricondivisione: RID }), 'Ricondividi').length, 0, '"Ricondividi" su una ricondivisione')
  const [b] = tasti(voce({}), 'Ricondividi')
  assert.ok(b)
  chiamate.length = 0
  await b.on.click({ stopPropagation() {} })
  assert.equal(chiamate.length, 0, 'ricondiviso al primo tocco')
  await b.on.click({ stopPropagation() {} })
  assert.equal(JSON.stringify(chiamate), JSON.stringify([['ricondividi', { id: ID }]]))
})

test('I-73 (3): "Togli" solo sulla tua ricondivisione, e cancella quella (non il post)', async () => {
  assert.equal(tasti(voce({ ricondiviso_da: ALTRO, ricondivisione: RID }), 'Togli').length, 0, '"Togli" sulla ricondivisione di un altro')
  const lista = new El('ol')
  const li = voce({ ricondiviso_da: IO, ricondivisione: RID })
  lista.append(li)
  assert.match(li.textContent, /l'hai ricondiviso/)
  const [b] = tasti(li, 'Togli')
  assert.ok(b)
  chiamate.length = 0
  await b.on.click({ stopPropagation() {} })
  await b.on.click({ stopPropagation() {} })
  assert.equal(JSON.stringify(chiamate), JSON.stringify([['cancella', { id: RID }]]))
  assert.equal(lista.children.length, 0)
})
