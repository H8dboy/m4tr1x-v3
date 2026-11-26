// @ts-check
'use strict'
/**
 * I-66 — Bloccare una persona resta sul tuo dispositivo e vale prima dell'interfaccia.
 *  (1) l'elenco vive solo in un file della cartella dati dell'app, scritto come i file privati del guscio, e non
 *      passa mai dalla rete: bloccati.rs non usa trasporto, feed, posta né firme; nessun elenco di "muti"
 *      pubblicato (kind 10000, NIP-51);
 *  (2) il filtro sta nel lato Rust, in OGNI comando che porta contenuti di altri all'interfaccia: feed, file
 *      (reels/film/musica), forum (elenco e discussione aperta con le risposte), messaggi privati;
 *  (3) chiavi non valide rifiutate, la tua identità non si blocca, mai oltre il tetto; un file rovinato si legge
 *      a metà senza fermare l'app (prove Rust `bloccati::prove`);
 *  (4) nell'interfaccia: "Blocca" sui contenuti degli altri, mai sui tuoi; chiede conferma con un secondo tocco
 *      (nessuna finestra del browser); il comando si chiama solo dopo la conferma.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const vm = require('vm')
const { spawnSync } = require('child_process')

const ROOT = path.join(__dirname, '..', '..')
const SHELL = path.join(ROOT, 'app', 'shell')
const read = (/** @type {string} */ p) => fs.readFileSync(p, 'utf8')
/** il codice senza i commenti: una spiegazione non deve far passare (o scattare) la prova */
const codice = (/** @type {string} */ p) => read(p).replace(/\/\/[^\n]*/g, '')

test('I-66 (1): l\'elenco dei bloccati non tocca mai la rete e non si pubblica', () => {
  const b = codice(path.join(SHELL, 'src', 'bloccati.rs'))
  for (const vietato of [/m4tr1x_transport/, /m4tr1x_feed/, /m4tr1x_posta/, /firma/, /Transport/, /pubblica\(/, /reqwest/, /TcpStream/])
    assert.ok(!vietato.test(b), `bloccati.rs usa ${vietato}`)
  assert.match(b, /crate::salva_privato\(/, 'l\'elenco non si scrive con salva_privato (file privato, scrittura atomica)')
  const lib = codice(path.join(SHELL, 'src', 'lib.rs'))
  assert.match(lib, /Bloccati::apri\(Some\(app\.path\(\)\.app_data_dir\(\)\?\.join\("bloccati"\)\)\)/, 'l\'elenco non sta nella cartella dati dell\'app')
  // nessun elenco di persone bloccate o mute pubblicato (NIP-51: kind 10000)
  for (const f of fs.readdirSync(path.join(SHELL, 'src')).filter(f => f.endsWith('.rs')))
    assert.ok(!/firma_con_la_mia_identita\(\s*&chiavi,\s*10000\b/.test(codice(path.join(SHELL, 'src', f))), `${f}: pubblica un elenco di muti`)
  // i comandi del blocco non parlano con la rete
  for (const cmd of ['bloccati', 'blocca', 'sblocca']) {
    const m = new RegExp(`#\\[tauri::command\\]\\s*fn ${cmd}\\([\\s\\S]*?\\n}\\n`).exec(lib)
    assert.ok(m, `comando ${cmd} non trovato (o diventato async)`)
    for (const v of ['tor', 'nodo', 'm4tr1x_feed', 'firma']) assert.ok(!new RegExp(`\\b${v}\\b`).test(m[0]), `il comando ${cmd} usa ${v}`)
  }
})

test('I-66 (2): il filtro sta nel lato Rust in ogni comando che porta contenuti di altri', () => {
  const lib = codice(path.join(SHELL, 'src', 'lib.rs'))
  const corpo = (/** @type {string} */ nome) => {
    const m = new RegExp(`async fn ${nome}\\([\\s\\S]*?\\n}\\n`).exec(lib)
    assert.ok(m, `comando ${nome} non trovato`)
    return m[0]
  }
  /** comando → filtri che devono esserci, PRIMA del ritorno */
  const attesi = {
    feed: [/f\.post\.retain\(\|p\| !bloccati\.contiene\(&p\.autore\)\)/],
    media: [/g\.media\.retain\(\|m\| !bloccati\.contiene\(&m\.autore\)\)/],
    forum: [/f\.discussioni\.retain\(\|d\| !bloccati\.contiene\(&d\.autore\)\)/],
    forum_discussione: [/d\.filter\(\|d\| !bloccati\.contiene\(&d\.discussione\.autore\)\)/, /d\.risposte\.retain\(\|r\| !bloccati\.contiene\(&r\.autore\)\)/],
    messaggi: [/p\.conversazioni\.retain\(\|c\| c\.identita\.as_deref\(\)\.is_none_or\(\|i\| !bloccati\.contiene\(i\)\)\)/],
  }
  for (const [nome, filtri] of Object.entries(attesi)) {
    const c = corpo(nome)
    for (const f of filtri) assert.match(c, f, `${nome}: manca il filtro dei bloccati ${f}`)
  }
  // ogni comando che restituisce contenuti altrui (tipi di api.rs con un autore) è nell'elenco qui sopra
  const conAutore = ['Feed', 'Galleria', 'Forum', 'DiscussioneAperta', 'Posta']
  const comandi = [...lib.matchAll(/#\[tauri::command\]\s*async fn (\w+)\([^)]*\)\s*->\s*Result<([^,]+),/g)]
    .filter(m => conAutore.some(t => new RegExp(`\\b${t}\\b`).test(m[2]))).map(m => m[1]).sort()
  assert.deepEqual(comandi, Object.keys(attesi).sort(), 'un comando porta contenuti di altri senza filtro dei bloccati')
})

const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })
test('I-66 (3): chiavi controllate, te stesso no, tetto, file rovinato (Rust)', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 1800000 }, () => {
  const app = path.join(ROOT, 'app')
  const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(app, 'Cargo.toml'), '-p', 'm4tr1x-shell', '--lib', 'bloccati::'],
    { cwd: app, encoding: 'utf8', timeout: 1800000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  for (const p of ['blocca_sblocca_e_resta_dopo_il_riavvio', 'chiavi_non_valide_e_se_stessi_rifiutati', 'file_rovinato_letto_a_meta_senza_errori', 'mai_oltre_il_massimo'])
    assert.match(out, new RegExp(`${p} \\.\\.\\. ok`), `manca o fallisce la prova ${p}`)
})

// ─── Interfaccia (app/ui/main.js con un DOM finto) ───────────────────────────
class El {
  constructor(/** @type {string} */ tag) { this.tag = tag; this.children = /** @type {El[]} */ ([]); this.parent = /** @type {El|null} */ (null); this.hidden = false; this.disabled = false; this._t = ''; this.on = /** @type {Record<string, Function>} */ ({}); this.className = ''; const cl = new Set(); this.classList = { add: (/** @type {string} */ c) => cl.add(c), remove: (/** @type {string} */ c) => cl.delete(c), contains: (/** @type {string} */ c) => cl.has(c), toggle: () => {} }; this.attrs = /** @type {Record<string, string>} */ ({}) }
  get textContent() { return this._t + this.children.map(c => c.textContent).join('') }
  set textContent(v) { this._t = String(v); this.children = [] }
  set innerHTML(_v) { throw new Error('innerHTML usato') }
  append(/** @type {Array<El|string>} */ ...c) {
    for (const y of c) { const x = typeof y === 'string' ? Object.assign(new El('#testo'), { _t: y }) : y; x.parent = this; this.children.push(x) }
  }
  remove() { if (this.parent) this.parent.children = this.parent.children.filter(c => c !== this) }
  setAttribute(/** @type {string} */ k, /** @type {string} */ v) { this.attrs[k] = v }
  getAttribute(/** @type {string} */ k) { return this.attrs[k] ?? null }
  addEventListener(/** @type {string} */ e, /** @type {Function} */ f) { this.on[e] = f }
  trova(/** @type {(e: El) => boolean} */ f) { /** @type {El[]} */ const out = []; const giro = (/** @type {El} */ e) => { if (f(e)) out.push(e); e.children.forEach(giro) }; giro(this); return out }
}

const IO = 'a'.repeat(64), ALTRO = 'b'.repeat(64)
/** @type {Array<[string, any]>} */ const chiamate = []
const ctx = vm.createContext({
  window: { __TAURI__: { core: { invoke: async (/** @type {string} */ cmd, /** @type {any} */ args) => { chiamate.push([cmd, args]); return cmd === 'bloccati' ? [] : null } } } },
  document: { createElement: (/** @type {string} */ t) => new El(t), createElementNS: (/** @type {string} */ _n, /** @type {string} */ t) => new El(t), addEventListener: () => {}, getElementById: () => null, querySelectorAll: () => [] },
  Date, Math, Number, String, Array, Set, Object, JSON, setTimeout, clearTimeout, clearInterval, setInterval, console,
})
vm.runInContext(fs.readFileSync(path.join(ROOT, 'app', 'ui', 'main.js'), 'utf8'), ctx)
vm.runInContext(`identita = { identity: '${IO}' }`, ctx)
const voce = (/** @type {string} */ autore) => /** @type {El} */ (ctx.voce({ id: 'c'.repeat(64), autore, creato: 1_760_000_000, testo: 'ciao', nome: null, bandierine: [] }))
const tasti = (/** @type {El} */ li) => li.trova(e => e.className === 'tasto-azione')

test('I-66 (4): "Blocca" sui contenuti degli altri, mai sui tuoi; segnati con il loro autore', () => {
  assert.equal(tasti(voce(IO)).length, 0, 'c\'è "Blocca" sul tuo post')
  const li = voce(ALTRO)
  assert.equal(tasti(li).length, 1, 'manca "Blocca" sul post di un altro')
  assert.equal(li.getAttribute('data-autore'), ALTRO)
  // una chiave che non è una chiave (finirebbe in un selettore): niente tasto, niente attributo
  const strano = voce('"] , * [x="')
  assert.equal(tasti(strano).length, 0)
  assert.equal(strano.getAttribute('data-autore'), null)
})

test('I-66 (4): il blocco parte solo al secondo tocco, senza finestre del browser', async () => {
  const js = codice(path.join(ROOT, 'app', 'ui', 'main.js'))
  assert.ok(!/\b(?:window\.)?(?:confirm|alert|prompt)\s*\(/.test(js), 'finestra di dialogo del browser nell\'interfaccia')
  const b = tasti(voce(ALTRO))[0]
  chiamate.length = 0
  await b.on.click({ stopPropagation() {} })
  assert.equal(chiamate.filter(c => c[0] === 'blocca').length, 0, 'bloccato al primo tocco')
  assert.ok(b.classList.contains('armato'))
  await b.on.click({ stopPropagation() {} })
  assert.equal(JSON.stringify(chiamate.filter(c => c[0] === 'blocca')), JSON.stringify([['blocca', { chiave: ALTRO }]]))
})
