// @ts-check
'use strict'
/**
 * I-72 — Cerca in tutto il nodo, senza aprire niente di privato. La parola va al nodo via Tor (NIP-50): la privacy
 * da proteggere è quella di chi scrive, non di chi cerca (fondatore, 26/11/2026).
 *  (1) nodo: si cerca solo nei contenuti pubblici (kind 0, 1, 1063 dichiarati nel filtro), parola da 1 a 100
 *      caratteri; % e _ presi alla lettera (non "tutto"); niente buste dei messaggi, niente segnalazioni;
 *  (2) app: la parola va nel filtro; il lato Rust tiene solo i risultati che contengono davvero la parola;
 *  (3) interfaccia: scrivendo si filtra ciò che è sullo schermo senza chiamare niente; con Invio si chiede al nodo.
 */
const { test, before, after } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const vm = require('vm')
const protocol = require('../../packages/protocol')
const { startTestNode, wsOpen, wsExchange, testIdentity, NET } = require('../helpers/node')

const ROOT = path.join(__dirname, '..', '..')
const UI = fs.readFileSync(path.join(ROOT, 'app', 'ui', 'main.js'), 'utf8')
const anna = testIdentity()
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
/** @param {any} filtro @returns {Promise<{ eventi: any[], chiuso: string|null }>} */
async function cerca(filtro) {
  const r = await scambia(['REQ', 'c', filtro], m => m[0] === 'EOSE' || m[0] === 'CLOSED' || m[0] === 'NOTICE')
  const fine = r.at(-1)
  return { eventi: r.filter(m => m[0] === 'EVENT').map(m => m[2]), chiuso: fine && fine[0] !== 'EOSE' ? String(fine[2] ?? fine[1]) : null }
}
const c = (/** @type {number} */ kind, /** @type {string} */ content, /** @type {string[][]} */ tags = []) => protocol.buildEvent({ kind, content, tags, created_at: Math.floor(Date.now() / 1000) }, anna, NET)

test('I-72 (1): il nodo cerca solo nei contenuti pubblici, % e _ alla lettera', async () => {
  const caffe = c(1, 'Caffè a Tirana stasera')
  const altro = c(1, 'il nodo è su')
  const percento = c(1, 'sconto del 50% oggi')
  const busta = c(1059, 'tirana cifrato', [['p', 'a'.repeat(64)]])
  for (const e of [caffe, altro, percento, busta]) assert.equal(await manda(e), true)
  const r = await cerca({ kinds: [1], search: 'TIRANA' })
  assert.deepEqual(r.eventi.map(e => e.id), [caffe.id], 'ricerca sbagliata (maiuscole?)')
  assert.deepEqual((await cerca({ kinds: [1], search: '%' })).eventi.map(e => e.id), [percento.id], '% vale come "tutto"')
  assert.equal((await cerca({ kinds: [1], search: '_' })).eventi.length, 0, '_ vale come "un carattere qualsiasi"')
  for (const [filtro, perche] of [[{ search: 'tirana' }, 'senza kind'], [{ kinds: [1059], search: 'tirana' }, 'nelle buste dei messaggi'], [{ kinds: [1, 1984], search: 'x' }, 'nelle segnalazioni'],
    [{ kinds: [1], search: '' }, 'parola vuota'], [{ kinds: [1], search: 'x'.repeat(101) }, 'oltre 100 caratteri'], [{ kinds: [1], search: 7 }, 'parola non testo']]) {
    const r = await cerca(filtro)
    assert.equal(r.eventi.length, 0, `cercato ${perche}`)
    assert.ok(r.chiuso, `filtro accettato: ${perche}`)
  }
})

test('I-72 (2): la parola va nel filtro e il lato Rust ricontrolla i risultati', () => {
  const lib = fs.readFileSync(path.join(ROOT, 'app', 'shell', 'src', 'lib.rs'), 'utf8').replace(/\/\/[^\n]*/g, '')
  const corpo = (/** @type {string} */ f) => /** @type {RegExpExecArray} */ (new RegExp(`async fn ${f}\\([\\s\\S]*?\\n}\\n`).exec(lib))[0]
  assert.match(corpo('feed'), /f\.post\.retain\(\|p\| contiene_parola\(&\[&p\.testo\], q\)\)/, 'feed: risultati non ricontrollati')
  assert.match(corpo('media'), /g\.media\.retain\(\|m\| contiene_parola\(&\[&m\.testo, &m\.file\.titolo\], q\)\)/, 'media: risultati non ricontrollati')
  assert.match(corpo('forum'), /f\.discussioni\.retain\(\|d\| contiene_parola\(&\[&d\.titolo, &d\.testo\], q\)\)/, 'forum: risultati non ricontrollati')
  const crate = fs.readFileSync(path.join(ROOT, 'app', 'feed', 'src', 'lib.rs'), 'utf8')
  assert.match(crate, /f\["search"\] = json!\(q\)/, 'la parola non va al nodo')
})

// ─── Interfaccia ─────────────────────────────────────────────────────────────
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

test('I-72 (3): scrivendo si filtra lo schermo senza chiamare niente', () => {
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
  assert.equal(chiamate.length, 0, `filtrare ha chiamato il lato Rust: ${chiamate}`)
  campo.value = '  tirana  '
  assert.equal(ctx.parolaCercata('cerca-feed'), 'tirana')
  campo.value = '   '
  assert.equal(ctx.parolaCercata('cerca-feed'), null, 'parola vuota mandata al nodo')
})

test('I-72 (3): con Invio si chiede al nodo, e il filtro si riapplica dopo', () => {
  for (const [cmd, id] of [['feed', 'cerca-feed'], ['media', 'cerca-media'], ['forum', 'cerca-forum']]) {
    assert.ok(new RegExp(`invoke\\('${cmd}', \\{[^}]*cerca: parolaCercata\\('${id}'\\)`).test(UI), `${cmd}: la parola non va al lato Rust`)
  }
  assert.match(UI, /addEventListener\('change', \(\) => ricarica\[id\]\(\)\)/, 'Invio non chiede al nodo')
  for (const [lista, campo] of [['f.post.map(voce)', 'cerca-feed'], ['lista.map(voceMedia)', 'cerca-media'], ['lista.map(voceDiscussione)', 'cerca-forum']]) {
    const i = UI.indexOf(lista)
    assert.ok(i > 0 && UI.slice(i, i + 200).includes(`filtraLista('${campo}')`), `${campo}: il filtro non si riapplica dopo l'aggiornamento`)
  }
})
