// @ts-check
'use strict'
/**
 * I-67 — Una segnalazione riguarda un CONTENUTO, mai una persona, e resta nel nodo che la riceve.
 *  (1) il nodo accetta solo la forma dell'app: un solo tag `e` con id e categoria dell'elenco chiuso, scadenza entro
 *      90 giorni, nessun altro tag (niente `p`: non si segnala una persona), testo breve;
 *  (2) nessuna richiesta la restituisce (per kind, id, autore, tag `e`, o senza filtro: così nemmeno i peer la
 *      copiano) e nessuna sottoscrizione aperta la riceve: chi è segnalato non sa chi l'ha segnalato;
 *  (3) il nodo la conta per il proprietario (/admin/bandierine: persone diverse, mai chi) e la cancella alla scadenza;
 *  (4) nell'app: la firma passa da `firma_con_la_mia_identita` con i tag di `tag_segnalazione` (prove Rust), il
 *      motivo si sceglie da un elenco chiuso, il tasto non compare senza identità né sui tuoi contenuti.
 */
const { test, before, after } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const vm = require('vm')
const { spawnSync } = require('child_process')
const protocol = require('../../packages/protocol')
const { CATEGORIE } = require('../../packages/moderazione')
const { startTestNode, request, wsOpen, wsExchange, testIdentity, NET } = require('../helpers/node')

const ROOT = path.join(__dirname, '..', '..')
const [anna, bruno, carla] = [testIdentity(), testIdentity(), testIdentity()]
const ora = () => Math.floor(Date.now() / 1000)
const scade = (/** @type {number} */ s = 86400) => ['expiration', String(ora() + s)]
/** @type {Awaited<ReturnType<typeof startTestNode>>} */ let t
/** @type {any} */ let post

before(async () => { t = await startTestNode({}) })
after(() => t.close())

/** @param {any} e */
async function manda(e) {
  const ws = await wsOpen(t.node.port)
  const r = await wsExchange(ws, ['EVENT', e], m => m[0] === 'OK')
  ws.close()
  return /** @type {[string, string, boolean, string]} */ (r.at(-1))
}
/** Tutti gli eventi che una richiesta restituisce. @param {any[]} filtri */
async function chiedi(...filtri) {
  const ws = await wsOpen(t.node.port)
  const r = await wsExchange(ws, ['REQ', 'q', ...filtri], m => m[0] === 'EOSE' || m[0] === 'CLOSED')
  ws.close()
  return r.filter(m => m[0] === 'EVENT').map(m => m[2])
}
const segnalazione = (/** @type {any} */ chi, /** @type {string[][]} */ tags, content = '') => protocol.buildEvent({ kind: 1984, content, tags }, chi, NET)

test('I-67 (1): il nodo accetta solo segnalazioni su UN contenuto, con categoria e scadenza', async () => {
  post = protocol.buildEvent({ kind: 1, content: 'un post qualunque', tags: [] }, anna, NET)
  assert.equal((await manda(post))[2], true)
  const id = post.id
  const brutte = [
    [[['e', id, 'spam']], 'senza scadenza'],
    [[['e', id, 'spam'], scade(200 * 86400)], 'scadenza troppo lontana'],
    [[['e', id, 'persona'], scade()], 'categoria fuori elenco'],
    [[['e', id], scade()], 'senza categoria'],
    [[['e', id, 'spam'], ['e', 'b'.repeat(64), 'spam'], scade()], 'due contenuti'],
    [[['e', id, 'spam'], ['p', anna.publicKeyHex || 'a'.repeat(64)], scade()], 'nomina una persona (p)'],
    [[['p', 'a'.repeat(64), 'spam'], scade()], 'solo una persona'],
    [[['e', 'XYZ', 'spam'], scade()], 'id non valido'],
    [[['e', id, 'spam'], ['t', 'qualcosa'], scade()], 'tag in più'],
  ]
  for (const [tags, perche] of brutte) {
    const r = await manda(segnalazione(bruno, /** @type {string[][]} */ (tags)))
    assert.equal(r[2], false, `accettata una segnalazione ${perche}`)
    assert.match(r[3], /^invalid: segnalazione/, perche)
  }
  assert.equal((await manda(segnalazione(bruno, [['e', id, 'spam'], scade()], 'x'.repeat(501))))[2], false, 'testo lungo accettato')
  for (const c of CATEGORIE) assert.equal((await manda(segnalazione(carla, [['e', id, c], scade()])))[2], true, `categoria ${c} rifiutata`)
})

test('I-67 (2): nessuna richiesta restituisce le segnalazioni, nessuna sottoscrizione le riceve', async () => {
  // una sottoscrizione aperta su tutto (come farebbe chi è segnalato, o un peer)
  const spia = await wsOpen(t.node.port)
  /** @type {any[]} */ const visti = []
  spia.on('message', d => { const m = JSON.parse(d.toString()); if (m[0] === 'EVENT') visti.push(m[2]) })
  await wsExchange(spia, ['REQ', 'tutto', {}, { kinds: [1984] }, { '#e': [post.id] }], m => m[0] === 'EOSE')
  const s = segnalazione(bruno, [['e', post.id, 'truffa'], scade()])
  assert.equal((await manda(s))[2], true)
  const altro = protocol.buildEvent({ kind: 1, content: 'dopo la segnalazione', tags: [] }, anna, NET)
  assert.equal((await manda(altro))[2], true)
  await new Promise(r => setTimeout(r, 300))
  assert.ok(visti.some(e => e.id === altro.id), 'la sottoscrizione non funziona (la prova non vedrebbe niente)')
  assert.ok(!visti.some(e => e.kind === 1984), 'una segnalazione è arrivata a una sottoscrizione')
  spia.close()
  for (const f of [{}, { kinds: [1984] }, { ids: [s.id] }, { ids: [s.id.slice(0, 8)] }, { authors: [s.pubkey] }, { '#e': [post.id] }, { kinds: [1984], authors: [s.pubkey], '#e': [post.id] }]) {
    const ev = await chiedi(f)
    assert.ok(!ev.some(e => e.kind === 1984), `la richiesta ${JSON.stringify(f).slice(0, 80)} restituisce segnalazioni`)
  }
  assert.ok((await chiedi({ ids: [post.id] })).some(e => e.id === post.id), 'le richieste non funzionano (la prova non vedrebbe niente)')
})

test('I-67 (3): il proprietario vede quante persone hanno segnalato, mai chi; la scadenza cancella', async () => {
  const r = await request(t.node.port, '/admin/bandierine', { headers: { 'X-Admin-Key': t.node.adminKey } })
  assert.equal(r.status, 200)
  const c = JSON.parse(r.body).contenuti.find((/** @type {any} */ x) => x.id === post.id)
  assert.ok(c, 'il contenuto segnalato non compare al proprietario')
  assert.equal(c.segnalazioni, 2, 'persone diverse contate male (bruno e carla)')
  for (const chi of [bruno, carla]) assert.ok(!r.body.includes(protocol.buildEvent({ kind: 1, content: '', tags: [] }, chi, NET).pubkey), 'il proprietario vede chi ha segnalato')
  // una segnalazione che scade tra poco sparisce con la pulizia del nodo
  const breve = segnalazione(anna, [['e', post.id, 'spam'], scade(2)])
  assert.equal((await manda(breve))[2], true)
  await new Promise(r => setTimeout(r, 3100))
  t.node.relay.purgeExpired()
  const resta = t.node.db.prepare('SELECT COUNT(*) AS n FROM events WHERE id = ?').get(breve.id)
  assert.equal(/** @type {any} */ (resta).n, 0, 'la segnalazione scaduta resta nel nodo')
})

const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })
test('I-67 (4): l\'app firma solo segnalazioni su un contenuto (Rust e guscio)', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 1800000 }, () => {
  const app = path.join(ROOT, 'app')
  const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(app, 'Cargo.toml'), '-p', 'm4tr1x-protocol', '--lib', 'prove_segnalazione'],
    { cwd: app, encoding: 'utf8', timeout: 1800000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  for (const p of ['una_segnalazione_nomina_solo_il_contenuto', 'id_e_categorie_fuori_elenco_rifiutati'])
    assert.match(out, new RegExp(`${p} \\.\\.\\. ok`), `manca o fallisce la prova ${p}`)
  // stesso elenco di categorie nel protocollo dell'app e nella moderazione del nodo
  const proto = fs.readFileSync(path.join(app, 'protocol', 'src', 'lib.rs'), 'utf8')
  const m = /CATEGORIE_SEGNALAZIONE: \[&str; \d+\] = \[([^\]]*)\]/.exec(proto)
  assert.ok(m, 'CATEGORIE_SEGNALAZIONE non trovato')
  assert.deepEqual([...m[1].matchAll(/"([^"]+)"/g)].map(x => x[1]).sort(), [...CATEGORIE].sort(), 'categorie diverse tra app e nodo')
  const lib = fs.readFileSync(path.join(app, 'shell', 'src', 'lib.rs'), 'utf8').replace(/\/\/[^\n]*/g, '')
  const cmd = /async fn segnala\([\s\S]*?\n}\n/.exec(lib)
  assert.ok(cmd, 'comando segnala non trovato')
  assert.match(cmd[0], /m4tr1x_protocol::tag_segnalazione\(&id, &categoria, ora\)/, 'i tag della segnalazione non vengono da tag_segnalazione')
  assert.match(cmd[0], /firma_con_la_mia_identita\(&chiavi, m4tr1x_protocol::KIND_SEGNALAZIONE, "", &tags\)/, 'la segnalazione porta altro (testo o tag)')
})

// ─── Interfaccia (app/ui/main.js con un DOM finto) ───────────────────────────
class El {
  constructor(/** @type {string} */ tag) { this.tag = tag; this.children = /** @type {El[]} */ ([]); this.parent = /** @type {El|null} */ (null); this.hidden = false; this.disabled = false; this.value = ''; this._t = ''; this.on = /** @type {Record<string, Function>} */ ({}); this.className = ''; this.classList = { add: () => {}, remove: () => {}, contains: () => false, toggle: () => {} }; this.attrs = /** @type {Record<string, string>} */ ({}) }
  get textContent() { return this._t + this.children.map(c => c.textContent).join('') }
  set textContent(v) { this._t = String(v); this.children = [] }
  set innerHTML(_v) { throw new Error('innerHTML usato') }
  append(/** @type {Array<El|string>} */ ...c) {
    for (const y of c) { const x = typeof y === 'string' ? Object.assign(new El('#testo'), { _t: y }) : y; x.parent = this; this.children.push(x); if (this.tag === 'select' && this.value === '') this.value = x.value }
  }
  remove() { if (this.parent) this.parent.children = this.parent.children.filter(c => c !== this) }
  setAttribute(/** @type {string} */ k, /** @type {string} */ v) { this.attrs[k] = v }
  getAttribute(/** @type {string} */ k) { return this.attrs[k] ?? null }
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
const voce = (/** @type {string} */ autore) => /** @type {El} */ (ctx.voce({ id: ID, autore, creato: 1_760_000_000, testo: 'ciao', nome: null, bandierine: [] }))
const segnala = (/** @type {El} */ li) => li.trova(e => e.className === 'tasto-azione' && e.textContent === 'Segnala')

test('I-67 (4): "Segnala" solo con un\'identità e solo sui contenuti degli altri; motivo da un elenco chiuso', async () => {
  vm.runInContext('identita = null', ctx)
  assert.equal(segnala(voce(ALTRO)).length, 0, '"Segnala" senza identità (non si potrebbe firmare)')
  vm.runInContext(`identita = { identity: '${IO}' }`, ctx)
  assert.equal(segnala(voce(IO)).length, 0, '"Segnala" sul tuo contenuto')
  const li = voce(ALTRO)
  assert.equal(segnala(li).length, 1)
  const opzioni = li.trova(e => e.tag === 'option').map(o => o.value)
  assert.deepEqual([...opzioni].sort(), [...CATEGORIE].sort(), 'i motivi nell\'interfaccia non sono l\'elenco chiuso')
  const invia = li.trova(e => e.tag === 'button' && e.textContent === 'Invia segnalazione')[0]
  const scelta = li.trova(e => e.tag === 'select')[0]
  chiamate.length = 0
  scelta.value = 'qualcosa-di-inventato'
  await invia.on.click({ stopPropagation() {} })
  assert.equal(chiamate.length, 0, 'segnalazione inviata con un motivo fuori elenco')
  scelta.value = 'truffa'
  await invia.on.click({ stopPropagation() {} })
  assert.equal(JSON.stringify(chiamate), JSON.stringify([['segnala', { id: ID, categoria: 'truffa' }]]))
})
