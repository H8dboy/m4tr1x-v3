// @ts-check
'use strict'
/**
 * I-69 — "Mi piace": firmato nel lato Rust con la tua identità, nomina solo il contenuto; si conta solo se verificato,
 * una persona una volta, solo per i contenuti mostrati; si toglie cancellando la TUA reazione (I-68).
 *  (1) nodo: la reazione si pubblica e si rilegge per #e; la sua cancellazione la toglie e non la fa rientrare;
 *  (2) Rust: `tag_mi_piace` (forma), `mi_piace_a` e `conta_mi_piace` (verificati, una per persona, la tua = la più recente);
 *  (3) app: il comando firma KIND_REAZIONE "+" con `tag_mi_piace`; il conto arriva dal lato Rust;
 *  (4) interfaccia: ♡ → `mi_piace`, ♥ → `cancella` sull'id della TUA reazione; senza identità solo il conto.
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
const [anna, bruno] = [testIdentity(), testIdentity()]
/** @type {Awaited<ReturnType<typeof startTestNode>>} */ let t
before(async () => { t = await startTestNode() })
after(() => t.close())

/** @param {any} e */
async function manda(e) {
  const ws = await wsOpen(t.node.port)
  const r = await wsExchange(ws, ['EVENT', e], m => m[0] === 'OK')
  ws.close()
  return /** @type {[string, string, boolean, string]} */ (r.at(-1))
}
/** @param {any} filtro */
async function chiedi(filtro) {
  const ws = await wsOpen(t.node.port)
  const r = await wsExchange(ws, ['REQ', 's', filtro], m => m[0] === 'EOSE')
  ws.close()
  return r.filter(m => m[0] === 'EVENT').map(m => m[2])
}
const c = (/** @type {any} */ chi, /** @type {number} */ kind, /** @type {string[][]} */ tags = [], content = '') => protocol.buildEvent({ kind, content, tags, created_at: Math.floor(Date.now() / 1000) }, chi, NET)

test('I-69 (1): il mi piace si rilegge per contenuto e si toglie cancellandolo', async () => {
  const post = c(bruno, 1, [], 'un post')
  assert.equal((await manda(post))[2], true)
  const piace = c(anna, 7, [['e', post.id]], '+')
  assert.equal((await manda(piace))[2], true)
  assert.deepEqual((await chiedi({ kinds: [7], '#e': [post.id] })).map(e => e.id), [piace.id])
  assert.equal((await manda(c(anna, 5, [['e', piace.id], ['k', '7']])))[2], true)
  assert.equal((await chiedi({ kinds: [7], '#e': [post.id] })).length, 0, 'il mi piace tolto si rilegge ancora')
  assert.equal((await manda(piace))[2], false, 'il mi piace tolto è rientrato')
})

const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })
test('I-69 (2-3): conto verificato nel lato Rust, firma con tag_mi_piace', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 1800000 }, () => {
  const app = path.join(ROOT, 'app')
  for (const [args, prova] of [[['-p', 'm4tr1x-feed', '--test', 'mi_piace'], 'solo_reazioni_valide_una_per_persona'], [['-p', 'm4tr1x-protocol', '--lib', 'prove_mi_piace'], 'mi_piace_solo_nella_forma_giusta']]) {
    const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(app, 'Cargo.toml'), ...args],
      { cwd: app, encoding: 'utf8', timeout: 1800000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
    const out = (r.stdout || '') + (r.stderr || '')
    assert.equal(r.status, 0, out.slice(-4000))
    assert.match(out, new RegExp(`${prova} \\.\\.\\. ok`), `manca o fallisce la prova ${prova}`)
  }
  const lib = fs.readFileSync(path.join(app, 'shell', 'src', 'lib.rs'), 'utf8').replace(/\/\/[^\n]*/g, '')
  const cmd = /async fn mi_piace\([\s\S]*?\n}\n/.exec(lib)
  assert.ok(cmd, 'comando mi_piace non trovato')
  assert.match(cmd[0], /m4tr1x_protocol::tag_mi_piace\(&id\)/)
  assert.match(cmd[0], /firma_con_la_mia_identita\(&chiavi, m4tr1x_protocol::KIND_REAZIONE, m4tr1x_protocol::MI_PIACE, &tags\)/)
  for (const f of ['feed', 'media', 'forum_discussione'])
    assert.match(/** @type {RegExpExecArray} */ (new RegExp(`async fn ${f}\\([\\s\\S]*?\\n}\\n`).exec(lib))[0], /con_reazioni\(/, `${f}: il conto non passa dal lato Rust`)
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
const IO = 'a'.repeat(64), ALTRO = 'b'.repeat(64), ID = 'c'.repeat(64), REAZ = 'd'.repeat(64)
/** @type {Array<[string, any]>} */ const chiamate = []
const ctx = vm.createContext({
  window: { __TAURI__: { core: { invoke: async (/** @type {string} */ cmd, /** @type {any} */ args) => { chiamate.push([cmd, args]); return cmd === 'mi_piace' ? REAZ : null } } } },
  document: { createElement: (/** @type {string} */ t) => new El(t), createElementNS: (/** @type {string} */ _n, /** @type {string} */ t) => new El(t), addEventListener: () => {}, getElementById: () => null, querySelectorAll: () => [] },
  Date, Math, Number, String, Array, Set, Object, JSON, setTimeout, clearTimeout, clearInterval, setInterval, console,
})
vm.runInContext(fs.readFileSync(path.join(ROOT, 'app', 'ui', 'main.js'), 'utf8'), ctx)
const voce = (/** @type {any} */ extra) => /** @type {El} */ (ctx.voce({ id: ID, autore: ALTRO, creato: 1_760_000_000, testo: 'ciao', nome: null, bandierine: [], ...extra }))
const cuore = (/** @type {El} */ li) => li.trova(e => e.className === 'tasto-azione mi-piace')[0]

test('I-69 (4): senza identità si vede solo il conto', () => {
  vm.runInContext('identita = null', ctx)
  const b = cuore(voce({ mi_piace: 3 }))
  assert.ok(b, 'manca il conto dei mi piace')
  assert.equal(b.textContent, '♡ 3')
  assert.equal(b.disabled, true, 'senza identità il tasto funziona')
})

test('I-69 (4): ♡ mette il mi piace, ♥ toglie la TUA reazione', async () => {
  vm.runInContext(`identita = { identity: '${IO}' }`, ctx)
  const b = cuore(voce({ mi_piace: 2, mio_mi_piace: null }))
  chiamate.length = 0
  await b.on.click({ stopPropagation() {} })
  assert.equal(JSON.stringify(chiamate), JSON.stringify([['mi_piace', { id: ID }]]))
  assert.equal(b.textContent, '♥ 3')
  await b.on.click({ stopPropagation() {} })
  assert.equal(JSON.stringify(chiamate[1]), JSON.stringify(['cancella', { id: REAZ }]), 'non cancella la reazione giusta')
  assert.equal(b.textContent, '♡ 2')
  // una reazione "mia" con un id storto non si cancella
  const s = cuore(voce({ mi_piace: 1, mio_mi_piace: '<x>' }))
  assert.equal(s.textContent, '♡ 1')
})
