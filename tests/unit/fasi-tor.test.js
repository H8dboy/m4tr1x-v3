// @ts-check
'use strict'
/**
 * Tutte le prove con Tor che RAGGIUNGONO un onion parlano la stessa lingua (fondatore, 20/10/2026):
 * l'errore dice `fase "pubblicazione"` o `fase "rete"` quando è la rete Tor (il runner li rilancia una
 * volta), e un'altra fase quando è il nostro codice (mai rilanciato). Una prova nuova che raggiunge un
 * onion va aggiunta qui; I-59 non raggiunge onion (porta di controllo, cookie, log): un suo rosso è codice.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')

const ROOT = path.resolve(__dirname, '..', '..')
const read = (/** @type {string} */ p) => fs.readFileSync(path.join(ROOT, p), 'utf8')

const PROVE = {
  'tests/integration/two-nodes.test.js': ['pubblicazione', 'rete', 'evento', 'file'],
  'tests/integration/feed-via-tor.test.js': ['pubblicazione', 'rete', 'tor', 'relay', 'feed', 'invio', 'firma'],
  'app/transport/tests/inv_37_tor_process.rs': ['pubblicazione', 'rete', 'tor', 'eco'],
}

test('le prove che raggiungono un onion scrivono la fase', () => {
  const src = read('tests/integration/two-nodes.test.js')
  // la prova dei due nodi costruisce il messaggio dal nome della fase
  assert.match(src, /fase "\$\{nome\}" fallita/)
  for (const f of ['pubblicazione', 'rete', 'evento', 'file']) assert.match(src, new RegExp(`fase\\('${f}'`), `two-nodes: manca la fase ${f}`)
  for (const [file, fasi] of Object.entries(PROVE).filter(([f]) => !f.includes('two-nodes'))) {
    const s = read(file)
    // JS: `fase "rete"` scritto o il nome tra apici; Rust: `fase \"{fase}\"` con il nome come stringa "rete"
    const rust = file.endsWith('.rs') && s.includes('fase \\"{fase}\\"')
    for (const f of fasi) assert.ok(s.includes(`fase "${f}"`) || s.includes(`fase \\"${f}\\"`) || s.includes(`'${f}'`) || (rust && s.includes(`"${f}"`)), `${file}: manca la fase ${f}`)
  }
})

test('ogni prova che usa M4TR1X_TOR_BIN e un onion è nell\'elenco (o è I-59)', () => {
  const walk = (/** @type {string} */ d) => fs.readdirSync(path.join(ROOT, d), { withFileTypes: true })
    .flatMap(e => e.isDirectory() ? walk(path.join(d, e.name)) : [path.join(d, e.name).replace(/\\/g, '/')])
  const candidati = [...walk('tests'), ...walk('app/transport/tests')]
    .filter(f => /\.(test\.js|rs)$/.test(f))
    .filter(f => { const s = read(f); return s.includes('M4TR1X_TOR_BIN') && /onion/i.test(s) && /connect_onion|\.onion|node\.onion/.test(s) })
  const ammessi = new Set([...Object.keys(PROVE), 'tests/invariants/inv-59-node-tor-control.test.js', 'tests/invariants/inv-37-tor-process.test.js', 'tests/unit/fasi-tor.test.js'])
  for (const f of candidati) assert.ok(ammessi.has(f), `${f} raggiunge un onion con tor vero ma non è nell'elenco delle fasi`)
})
