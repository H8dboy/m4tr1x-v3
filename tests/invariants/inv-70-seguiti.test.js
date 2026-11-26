// @ts-check
'use strict'
/**
 * I-70 — Chi segui: l'elenco sta sul dispositivo (file privato) e non si pubblica (niente kind 3). "Chi segui" chiede
 * al nodo, via Tor, i post di quelle persone (`authors`): la privacy da proteggere è quella di chi scrive, non di chi
 * legge (fondatore, 26/11/2026). Il lato Rust ricontrolla la risposta.
 *  (1) Rust: elenco controllato (chiavi valide, te stesso no), su file privato, svuotato con l'account;
 *  (2) guscio: la richiesta porta l'elenco; DOPO la lettura si tiene solo chi segui (il nodo può sbagliare); elenco
 *      vuoto = niente richiesta; nessun kind 3; bloccare smette di seguire, e chi è bloccato non si segue;
 *  (3) interfaccia: "Segui" solo su altri e con un'identità; al lato Rust passa solo un sì/no (e la parola cercata).
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const vm = require('vm')
const { spawnSync } = require('child_process')

const ROOT = path.join(__dirname, '..', '..')
const APP = path.join(ROOT, 'app')
const senzaCommenti = (/** @type {string} */ s) => s.replace(/\/\/[^\n]*/g, '')
const lib = senzaCommenti(fs.readFileSync(path.join(APP, 'shell', 'src', 'lib.rs'), 'utf8'))
const corpo = (/** @type {string} */ nome) => {
  const m = new RegExp(`fn ${nome}\\([\\s\\S]*?\\n}\\n`).exec(lib)
  assert.ok(m, `funzione ${nome} non trovata`)
  return m[0]
}

const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })
test('I-70 (1): elenco controllato, su file privato, svuotabile', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 1800000 }, () => {
  const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(APP, 'Cargo.toml'), '-p', 'm4tr1x-shell', '--lib', 'seguiti'],
    { cwd: APP, encoding: 'utf8', timeout: 1800000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  assert.match(out, /seguiti_solo_sul_dispositivo_e_te_stesso_no \.\.\. ok/)
  const mod = senzaCommenti(fs.readFileSync(path.join(APP, 'shell', 'src', 'seguiti.rs'), 'utf8'))
  assert.match(mod, /crate::salva_privato\(/, 'l\'elenco non si scrive come file privato')
  assert.ok(!/m4tr1x_feed|m4tr1x_protocol|pubblica\(|firma/.test(mod), 'il modulo di chi segui parla con la rete o firma')
})

test('I-70 (2): l\'elenco va al nodo nella richiesta, la risposta si ricontrolla, niente kind 3', { timeout: 1800000 }, () => {
  const feed = corpo('feed')
  assert.ok(feed.includes('autori: (solo_seguiti == Some(true)).then(|| seguiti.elenco())'), '"Chi segui" non chiede al nodo i post di chi segui')
  assert.ok(/is_some_and\(Vec::is_empty\) \{\s*return Ok/.test(feed), 'con l\'elenco vuoto si chiede lo stesso al nodo (darebbe il feed di tutti)')
  const lettura = feed.indexOf('leggi_richiesta_con_ripresa('), filtro = feed.indexOf('seguiti.contiene(&p.autore)')
  assert.ok(lettura > 0 && filtro > lettura, 'dopo la lettura non si ricontrolla chi segui')
  if (cargo.status === 0) {
    const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(APP, 'Cargo.toml'), '-p', 'm4tr1x-feed', '--test', 'feed', 'richiesta_al_nodo'],
      { cwd: APP, encoding: 'utf8', timeout: 1800000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
    assert.match((r.stdout || '') + (r.stderr || ''), /richiesta_al_nodo_con_chi_segui_e_parola \.\.\. ok/, 'il filtro per il nodo non porta chi segui')
  }
  for (const dir of ['shell', 'feed', 'protocol']) {
    for (const f of fs.readdirSync(path.join(APP, dir, 'src'))) {
      const src = senzaCommenti(fs.readFileSync(path.join(APP, dir, 'src', f), 'utf8'))
      assert.ok(!/KIND_CONTATTI|kind:\s*3\b|"kinds":\s*\[3\]/.test(src), `${dir}/${f}: elenco dei contatti pubblico (kind 3)`)
    }
  }
  assert.match(corpo('blocca'), /seguiti\.smetti\(&chiave\)/, 'bloccare non smette di seguire')
  assert.match(corpo('segui'), /bloccati\.contiene\(/, 'si può seguire chi è bloccato')
  for (const f of ['account_cancella', 'identita_dimentica']) assert.match(corpo(f), /seguiti\.svuota\(\)/, `${f}: l'elenco di chi segui resta`)
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
const IO = 'a'.repeat(64), ALTRO = 'b'.repeat(64), ID = 'c'.repeat(64)
/** @type {Array<[string, any]>} */ const chiamate = []
const ctx = vm.createContext({
  window: { __TAURI__: { core: { invoke: async (/** @type {string} */ cmd, /** @type {any} */ args) => { chiamate.push([cmd, args]); return cmd === 'seguiti' ? [ALTRO, '<x>'] : null } } } },
  document: { createElement: (/** @type {string} */ t) => new El(t), createElementNS: (/** @type {string} */ _n, /** @type {string} */ t) => new El(t), addEventListener: () => {}, getElementById: () => null, querySelectorAll: () => [] },
  Date, Math, Number, String, Array, Set, Object, JSON, setTimeout, clearTimeout, clearInterval, setInterval, console,
})
vm.runInContext(fs.readFileSync(path.join(ROOT, 'app', 'ui', 'main.js'), 'utf8'), ctx)
const voce = (/** @type {string} */ autore) => /** @type {El} */ (ctx.voce({ id: ID, autore, creato: 1_760_000_000, testo: 'ciao', nome: null, bandierine: [] }))
const tasto = (/** @type {El} */ li) => li.trova(e => e.className === 'tasto-azione segui')[0]

test('I-70 (3): "Segui" solo su altri, con un\'identità; segue e smette', async () => {
  vm.runInContext('identita = null', ctx)
  assert.equal(tasto(voce(ALTRO)), undefined, '"Segui" senza identità')
  vm.runInContext(`identita = { identity: '${IO}' }`, ctx)
  assert.equal(tasto(voce(IO)), undefined, '"Segui" su te stesso')
  await ctx.caricaSeguiti()
  assert.equal(vm.runInContext('seguiti.size', ctx), 1, 'una chiave storta dal lato Rust è finita nell\'elenco')
  const b = tasto(voce(ALTRO))
  assert.equal(b.textContent, 'Segui già ✓')
  chiamate.length = 0
  await b.on.click({ stopPropagation() {} })
  assert.equal(JSON.stringify(chiamate), JSON.stringify([['smetti_di_seguire', { chiave: ALTRO }]]))
  assert.equal(b.textContent, 'Segui')
  await b.on.click({ stopPropagation() {} })
  assert.equal(JSON.stringify(chiamate[1]), JSON.stringify(['segui', { chiave: ALTRO }]))
  assert.equal(b.textContent, 'Segui già ✓')
})

test('I-70 (3): al lato Rust passa solo un sì/no (e la parola cercata)', () => {
  const ui = fs.readFileSync(path.join(ROOT, 'app', 'ui', 'main.js'), 'utf8')
  const chiamateFeed = [...ui.matchAll(/invoke\('feed'(?:,\s*(\{[^}]*\}))?\)/g)].map(m => (m[1] || '').trim())
  assert.deepEqual(chiamateFeed, ["{ soloSeguiti, cerca: parolaCercata('cerca-feed') }"], `il feed riceve altro dall'interfaccia: ${JSON.stringify(chiamateFeed)}`)
  assert.match(ui, /let soloSeguiti = false/)
})
