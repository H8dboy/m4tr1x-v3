// @ts-check
'use strict'
/**
 * I-72 — Cercare non racconta niente: la ricerca avviene solo nell'interfaccia, tra contenuti già scaricati e
 * verificati. La parola cercata non va al nodo (niente NIP-50) e nemmeno al lato Rust (nessun `invoke`).
 *  (1) filtrare non chiama nessun comando; nasconde le voci senza la parola (testo o chiave), senza accenti né maiuscole;
 *  (2) i campi di ricerca non finiscono in nessuna chiamata a `invoke`; nessun filtro `search` nel crate del feed;
 *  (3) dopo un aggiornamento il filtro si riapplica.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const vm = require('vm')

const ROOT = path.join(__dirname, '..', '..')
const UI = fs.readFileSync(path.join(ROOT, 'app', 'ui', 'main.js'), 'utf8')

class El {
  constructor(/** @type {string} */ tag) { this.tag = tag; this.children = /** @type {El[]} */ ([]); this.hidden = false; this.value = ''; this._t = ''; this.attrs = /** @type {Record<string, string>} */ ({}); this.on = /** @type {Record<string, Function>} */ ({}) }
  get textContent() { return this._t + this.children.map(c => c.textContent).join('') }
  set textContent(v) { this._t = String(v); this.children = [] }
  set innerHTML(_v) { throw new Error('innerHTML usato') }
  append(/** @type {El[]} */ ...c) { this.children.push(...c) }
  setAttribute(/** @type {string} */ k, /** @type {string} */ v) { this.attrs[k] = v }
  getAttribute(/** @type {string} */ k) { return this.attrs[k] ?? null }
  addEventListener(/** @type {string} */ e, /** @type {Function} */ f) { this.on[e] = f }
}
const voce = (/** @type {string} */ testo, /** @type {string} */ autore) => { const li = new El('li'); li.textContent = testo; li.setAttribute('data-autore', autore); return li }
const lista = new El('ol'), campo = new El('input'), conta = new El('span')
lista.append(voce('Caffè a Tirana', 'a'.repeat(64)), voce('Il nodo è su', 'b'.repeat(64)), voce('ciao', 'c'.repeat(64)))
/** @type {string[]} */ const chiamate = []
const ctx = vm.createContext({
  window: { __TAURI__: { core: { invoke: async (/** @type {string} */ cmd) => { chiamate.push(cmd); return null } } } },
  document: { createElement: (/** @type {string} */ t) => new El(t), createElementNS: (/** @type {string} */ _n, /** @type {string} */ t) => new El(t), addEventListener: () => {}, getElementById: (/** @type {string} */ id) => ({ 'feed-lista': lista, 'cerca-feed': campo, 'cerca-feed-conta': conta })[id] || null, querySelectorAll: () => [] },
  Date, Math, Number, String, Array, Set, Object, JSON, setTimeout, clearTimeout, clearInterval, setInterval, console,
})
vm.runInContext(UI, ctx)

test('I-72 (1): si filtra in locale, senza chiamare niente, senza accenti né maiuscole', () => {
  chiamate.length = 0
  campo.value = 'CAFFE'
  ctx.filtraLista('cerca-feed')
  assert.deepEqual(lista.children.map(li => li.hidden), [false, true, true])
  assert.equal(conta.textContent, '1 trovati')
  campo.value = 'b'.repeat(10)
  ctx.filtraLista('cerca-feed')
  assert.deepEqual(lista.children.map(li => li.hidden), [true, false, true], 'la chiave dell\'autore non si cerca')
  campo.value = ''
  ctx.filtraLista('cerca-feed')
  assert.deepEqual(lista.children.map(li => li.hidden), [false, false, false])
  assert.equal(chiamate.length, 0, `cercare ha chiamato il lato Rust: ${chiamate}`)
})

test('I-72 (2): la parola cercata non finisce in nessuna chiamata, nessun search al nodo', () => {
  const corpo = /function filtraLista\([\s\S]*?\n}\n/.exec(UI)
  assert.ok(corpo && !/invoke\(/.test(corpo[0]), 'filtraLista chiama il lato Rust')
  for (const m of UI.matchAll(/invoke\([^)]*\)/g)) assert.ok(!/cerca/i.test(m[0]), `la ricerca passa a un comando: ${m[0]}`)
  for (const f of ['feed/src/lib.rs', 'shell/src/lib.rs', 'transport/src/http.rs']) {
    const p = path.join(ROOT, 'app', f)
    if (fs.existsSync(p)) assert.ok(!/"search"/.test(fs.readFileSync(p, 'utf8')), `${f}: ricerca chiesta al nodo`)
  }
})

test('I-72 (3): dopo un aggiornamento il filtro si riapplica', () => {
  for (const [lista, campo] of [['f.post.map(voce)', 'cerca-feed'], ['lista.map(voceMedia)', 'cerca-media'], ['lista.map(voceDiscussione)', 'cerca-forum']]) {
    const i = UI.indexOf(lista)
    assert.ok(i > 0, `manca ${lista}`)
    assert.ok(UI.slice(i, i + 200).includes(`filtraLista('${campo}')`), `${campo}: il filtro non si riapplica dopo l'aggiornamento`)
  }
})
