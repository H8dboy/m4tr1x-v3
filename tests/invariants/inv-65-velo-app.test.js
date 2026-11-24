// @ts-check
'use strict'
/**
 * I-65 nell'app: un contenuto con una bandierina del nodo si vede COPERTO da un velo con il motivo (in parole, da
 * un elenco chiuso) e si apre solo se lo chiedi; il possibile sfruttamento di minori non si apre. Il testo non
 * diventa mai markup. Se il nodo non dà le bandierine, il feed lo dice. Lato Rust: risposta del nodo controllata
 * campo per campo, bandierine attaccate solo al post giusto.
 * L'interfaccia vera (app/ui/main.js) gira qui con un DOM finto: si provano le funzioni che usa l'app.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const vm = require('vm')
const { spawnSync } = require('child_process')

const ROOT = path.join(__dirname, '..', '..')
const JS = fs.readFileSync(path.join(ROOT, 'app', 'ui', 'main.js'), 'utf8')

class El {
  constructor(/** @type {string} */ tag) { this.tag = tag; this.children = /** @type {El[]} */ ([]); this.parent = /** @type {El|null} */ (null); this.hidden = false; this._t = ''; this.on = /** @type {Record<string, Function>} */ ({}); this.className = ''; this.classList = { add: () => {} }; this.attrs = {} }
  get textContent() { return this._t + this.children.map(c => c.textContent).join('') }
  set textContent(v) { this._t = String(v); this.children = [] }
  set innerHTML(_v) { throw new Error('innerHTML usato') }
  append(/** @type {Array<El|string>} */ ...c) {
    for (const y of c) { const x = typeof y === 'string' ? Object.assign(new El('#testo'), { _t: y }) : y; x.parent = this; this.children.push(x) }
  }
  remove() { if (this.parent) this.parent.children = this.parent.children.filter(c => c !== this) }
  setAttribute(/** @type {string} */ k, /** @type {string} */ v) { /** @type {any} */ (this.attrs)[k] = v }
  addEventListener(/** @type {string} */ e, /** @type {Function} */ f) { this.on[e] = f }
  trova(/** @type {(e: El) => boolean} */ f) { /** @type {El[]} */ const out = []; const giro = (/** @type {El} */ e) => { if (f(e)) out.push(e); e.children.forEach(giro) }; giro(this); return out }
}
const ctx = vm.createContext({
  window: { __TAURI__: { core: { invoke: async () => null } } },
  document: { createElement: (/** @type {string} */ t) => new El(t), createElementNS: (/** @type {string} */ _n, /** @type {string} */ t) => new El(t), addEventListener: () => {}, getElementById: () => null, querySelectorAll: () => [] },
  Date, Math, Number, String, Array, Set, Object, JSON, setTimeout, clearInterval, setInterval, console,
})
vm.runInContext(JS, ctx)
const voce = (/** @type {any} */ p) => /** @type {El} */ (ctx.voce({ id: 'a'.repeat(64), autore: 'f'.repeat(64), creato: 1_760_000_000, testo: 'TESTO DEL POST <b>x</b>', nome: null, ...p }))
const testo = (/** @type {El} */ li) => li.trova(e => e.className === 'testo')[0]
const velo = (/** @type {El} */ li) => li.trova(e => e.className === 'velo')[0]

test('I-65 app: senza bandierine il post si vede, senza velo', () => {
  for (const bandierine of [[], undefined]) {
    const li = voce({ bandierine })
    assert.equal(testo(li).hidden, false)
    assert.equal(velo(li), undefined)
  }
})

test('I-65 app: con una bandierina il testo è coperto, il motivo è scritto, e si apre solo se lo chiedi', () => {
  const li = voce({ bandierine: [{ categoria: 'truffa', peso: 3, codice: true }, { categoria: 'spam', peso: 1, codice: false }] })
  assert.equal(testo(li).hidden, true, 'testo con bandierina visibile senza velo')
  const v = velo(li)
  assert.match(v.textContent, /Coperto dal nodo: possibile truffa, possibile spam · scritto in codice/)
  assert.ok(!v.textContent.includes('TESTO DEL POST'), 'il velo mostra il testo')
  const apri = v.trova(e => e.tag === 'button')[0]
  assert.ok(apri, 'manca "Mostra comunque"')
  apri.on.click()
  assert.equal(testo(li).hidden, false)
  assert.equal(velo(li), undefined, 'il velo resta dopo averlo aperto')
  // il testo resta testo: nessun markup (il DOM finto lancia se si usa innerHTML)
  assert.equal(testo(li).textContent, 'TESTO DEL POST <b>x</b>')
})

test('I-65 app: il possibile sfruttamento di minori non si apre; categorie sconosciute = "segnalato dal nodo"', () => {
  const li = voce({ bandierine: [{ categoria: 'sfruttamento-minori', peso: 3, codice: false }, { categoria: 'spam', peso: 1, codice: false }] })
  assert.equal(testo(li).hidden, true)
  assert.equal(velo(li).trova(e => e.tag === 'button').length, 0, 'si può aprire')
  assert.match(velo(li).textContent, /lo valuterà un moderatore/)
  const ignota = voce({ bandierine: [{ categoria: '<img src=x onerror=alert(1)>', peso: 1, codice: false }] })
  assert.match(velo(ignota).textContent, /Coperto dal nodo: segnalato dal nodo\./)
  assert.ok(!velo(ignota).textContent.includes('<img'), 'una categoria del nodo finisce nel testo')
})

test('I-65 app: forum e reel — titolo, anteprima e apertura sotto il velo finché non lo apri', () => {
  const disc = (/** @type {any} */ bandierine) => /** @type {El} */ (ctx.voceDiscussione({ id: 'a'.repeat(64), autore: 'f'.repeat(64), nome: null, titolo: 'TITOLO DISC', testo: 'TESTO DISC', creato: 1, risposte: 2, attivita: 1, bandierine }))
  const reel = (/** @type {any} */ bandierine) => /** @type {El} */ (ctx.voceMedia({ id: 'b'.repeat(64), autore: 'f'.repeat(64), nome: null, testo: 'TESTO REEL', creato: 1, bandierine,
    file: { sha256: 'e'.repeat(64), mime: 'video/mp4', dimensione: 1000, sezione: 'signal', titolo: 'TITOLO REEL', pezzi: [], misure: [], celle: false } }))
  for (const [cosa, fai, titolo] of /** @type {Array<[string, (b: any) => El, string]>} */ ([['discussione', disc, 'TITOLO DISC'], ['reel', reel, 'TITOLO REEL']])) {
    const tessera = (/** @type {El} */ li) => li.children.find(c => c.tag === 'button')
    const pulita = fai([])
    assert.equal(velo(pulita), undefined, `${cosa} senza bandierine coperta`)
    assert.equal(/** @type {El} */ (tessera(pulita)).hidden, false)
    const coperta = fai([{ categoria: 'spam', peso: 1, codice: false }])
    const t = /** @type {El} */ (tessera(coperta))
    assert.equal(t.hidden, true, `${cosa} con bandierina: titolo e apertura visibili`)
    assert.ok(!velo(coperta).textContent.includes(titolo), `${cosa}: il velo mostra il titolo`)
    assert.match(velo(coperta).textContent, /Coperto dal nodo: possibile spam\./)
    velo(coperta).trova(e => e.tag === 'button')[0].on.click()
    assert.equal(t.hidden, false, `${cosa}: «Mostra comunque» non apre`)
    const chiusa = fai([{ categoria: 'sfruttamento-minori', peso: 3, codice: false }])
    assert.equal(/** @type {El} */ (tessera(chiusa)).hidden, true)
    assert.equal(velo(chiusa).trova(e => e.tag === 'button').length, 0, `${cosa}: il possibile sfruttamento di minori si apre`)
  }
})

test('I-65 app: le risposte del forum passano dal velo come i post', () => {
  assert.match(JS, /document\.getElementById\('disc-risposte'\)\?\.replaceChildren\(\.\.\.d\.risposte\.map\(voce\)\)/, 'le risposte non passano da voce()')
})

test('I-65 app: se il nodo non dà le bandierine, feed, forum e reel lo dicono', () => {
  assert.match(JS, /f\.bandierine_disponibili \? '' : ' · bandierine del nodo non disponibili'/)
  assert.match(JS, /g\.bandierine_disponibili \? '' : ' · bandierine del nodo non disponibili'/)
  assert.match(JS, /d\.bandierine_disponibili \? '' : ' · bandierine del nodo non disponibili'/)
})

const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8' })
test('I-65 app (Rust): risposta del nodo controllata campo per campo, bandierine solo sul post giusto', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 1800000 }, () => {
  for (const [pacchetto, filtro, prova] of [
    ['m4tr1x-transport', 'http::', 'bandierine_solo_categoria_peso_e_codice_e_solo_per_gli_id_chiesti'],
    ['m4tr1x-shell', 'prove_bandierine', 'con_bandierine_attacca_solo_quelle_del_post_giusto'],
    ['m4tr1x-shell', 'prove_bandierine', 'forum_discussioni_risposte_e_schede_ricevono_solo_le_loro'],
  ]) {
    const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(ROOT, 'app', 'Cargo.toml'), '-p', pacchetto, '--lib', filtro], { encoding: 'utf8', maxBuffer: 64 << 20 })
    assert.equal(r.status, 0, `${pacchetto}: ${r.stdout.slice(-3000)}${r.stderr.slice(-3000)}`)
    assert.match(r.stdout, new RegExp(`test [\\w:]*${prova} \\.\\.\\. ok`), `${prova} non ha girato`)
  }
})
