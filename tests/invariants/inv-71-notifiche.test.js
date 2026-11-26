// @ts-check
'use strict'
/**
 * I-71 — Notifiche calcolate sul dispositivo. Il nodo non tiene "notifiche per te": l'app chiede i suoi contenuti
 * recenti e poi le risposte e i "mi piace" che li nominano, e li verifica nel lato Rust.
 *  (1) Rust: solo eventi verificati, solo sui TUOI contenuti, mai le tue reazioni o risposte, niente doppioni;
 *  (2) guscio: serve un'identità; chi hai bloccato non compare; "viste fino a" solo sul dispositivo (file privato),
 *      mai nel futuro; niente testo delle risposte nella notifica (si legge aprendo la discussione, con i veli);
 *  (3) interfaccia: solo textContent; voci storte scartate; "nuova" in grassetto.
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
test('I-71 (1): solo eventi verificati sui tuoi contenuti', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 1800000 }, () => {
  const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(APP, 'Cargo.toml'), '-p', 'm4tr1x-feed', '--test', 'notifiche'],
    { cwd: APP, encoding: 'utf8', timeout: 1800000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  assert.match(out, /solo_risposte_e_mi_piace_verificati_ai_miei_contenuti \.\.\. ok/)
})

test('I-71 (2): identità, bloccati, viste solo sul dispositivo, niente testo', () => {
  const n = corpo('notifiche')
  assert.match(n, /mia_chiave\(&chiavi\)\.ok_or\(/, 'notifiche senza identità')
  assert.match(n, /\.filter\(\|n\| !bloccati\.contiene\(&n\.autore\)\)/, 'chi hai bloccato compare nelle notifiche')
  const v = corpo('notifiche_viste')
  assert.match(v, /fino\.min\(ora\)/, '"viste fino a" può andare nel futuro (nasconderebbe le notifiche nuove)')
  assert.match(v, /salva_privato\(/, '"viste fino a" non sta in un file privato')
  const api = senzaCommenti(fs.readFileSync(path.join(APP, 'shell', 'src', 'api.rs'), 'utf8'))
  const tipo = /pub struct Notifica \{[\s\S]*?\n\}/.exec(api)
  assert.ok(tipo, 'tipo Notifica non trovato')
  assert.ok(!/testo|content|titolo/.test(tipo[0]), 'la notifica porta il testo della risposta (senza veli)')
})

// ─── Interfaccia ─────────────────────────────────────────────────────────────
class El {
  constructor(/** @type {string} */ tag) { this.tag = tag; this.children = /** @type {El[]} */ ([]); this.parent = /** @type {El|null} */ (null); this.hidden = false; this.disabled = false; this.title = ''; this._t = ''; this.on = /** @type {Record<string, Function>} */ ({}); this.className = ''; this.classList = { add: () => {}, remove: () => {}, contains: () => false, toggle: () => {} }; this.attrs = /** @type {Record<string, string>} */ ({}) }
  get textContent() { return this._t + this.children.map(c => c.textContent).join('') }
  set textContent(v) { this._t = String(v); this.children = [] }
  set innerHTML(_v) { throw new Error('innerHTML usato') }
  append(/** @type {Array<El|string>} */ ...c) { for (const y of c) { const x = typeof y === 'string' ? Object.assign(new El('#testo'), { _t: y }) : y; x.parent = this; this.children.push(x) } }
  replaceChildren(/** @type {El[]} */ ...c) { this.children = []; this.append(...c) }
  setAttribute(/** @type {string} */ k, /** @type {string} */ v) { this.attrs[k] = v }
  addEventListener(/** @type {string} */ e, /** @type {Function} */ f) { this.on[e] = f }
}
const A = 'a'.repeat(64), C = 'c'.repeat(64), R = 'e'.repeat(64)
const lista = new El('ol'), stato = new El('span')
/** @type {Array<[string, any]>} */ const chiamate = []
const risposta = [
  { tipo: 'risposta', id: R, autore: A, contenuto: C, creato: 1_760_000_100, nuova: true },
  { tipo: 'mi_piace', id: R, autore: A, contenuto: C, creato: 1_760_000_050, nuova: false },
  { tipo: 'altro', id: R, autore: A, contenuto: C, creato: 1, nuova: true },
  { tipo: 'mi_piace', id: R, autore: '<img>', contenuto: C, creato: 1, nuova: true },
]
const ctx = vm.createContext({
  window: { __TAURI__: { core: { invoke: async (/** @type {string} */ cmd, /** @type {any} */ args) => { chiamate.push([cmd, args]); return cmd === 'notifiche' ? risposta : null } } } },
  document: { createElement: (/** @type {string} */ t) => new El(t), createElementNS: (/** @type {string} */ _n, /** @type {string} */ t) => new El(t), addEventListener: () => {}, getElementById: (/** @type {string} */ id) => ({ 'notifiche-lista': lista, 'notifiche-stato': stato })[id] || null, querySelectorAll: () => [] },
  Date, Math, Number, String, Array, Set, Object, JSON, setTimeout, clearTimeout, clearInterval, setInterval, console,
})
vm.runInContext(fs.readFileSync(path.join(ROOT, 'app', 'ui', 'main.js'), 'utf8'), ctx)

test('I-71 (3): voci storte scartate, nuove segnate, viste fino all\'ultima', async () => {
  await ctx.aggiornaNotifiche()
  assert.equal(lista.children.length, 2, 'voci storte mostrate')
  assert.equal(lista.children[0].className, 'notifica nuova')
  assert.match(lista.children[0].textContent, /ha risposto alla tua discussione/)
  assert.match(lista.children[1].textContent, /ha messo ♥/)
  assert.equal(JSON.stringify(chiamate.filter(c => c[0] === 'notifiche_viste')), JSON.stringify([['notifiche_viste', { fino: 1_760_000_100 }]]))
})
