// @ts-check
'use strict'
/**
 * I-36 — Trasporto unico: solo il crate `app/transport` apre connessioni di rete,
 * e accetta solo onion v3 validati (ADR-0016).
 *
 * Attivo da F4 (parte statica):
 *  (1) nessun crate dell'app diverso da `transport` dichiara librerie di rete;
 *  (2) nessun sorgente Rust fuori da `app/transport/` usa socket o client HTTP;
 *  (3) l'interfaccia (app/ui) non fa richieste di rete: niente fetch, WebSocket,
 *      XMLHttpRequest, EventSource, sendBeacon, né risorse con URL esterni;
 *  (4) `connect_onion` accetta solo `&OnionV3` (nessun metodo verso host in chiaro).
 * Limite dichiarato: le dipendenze transitive di Tauri contengono codice di rete;
 * questo test garantisce che il NOSTRO codice non lo chiami. La prova dinamica
 * "nessun pacchetto esce fuori da Tor" è I-01 (namespace di rete, Linux).
 * Da completare: la stessa suite di prove gira su ArtiTransport quando esiste.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')

const ROOT = path.resolve(__dirname, '..', '..')
const APP = path.join(ROOT, 'app')
const read = (/** @type {string} */ p) => fs.readFileSync(p, 'utf8')
const walk = (/** @type {string} */ d) => fs.readdirSync(d, { withFileTypes: true })
  .filter(e => !['target', 'gen', 'node_modules'].includes(e.name))
  .flatMap(e => e.isDirectory() ? walk(path.join(d, e.name)) : [path.join(d, e.name)])

const NET_CRATES = ['reqwest', 'hyper', 'hyper-util', 'ureq', 'isahc', 'curl', 'attohttpc', 'surf', 'socket2', 'mio',
  'arti-client', 'tor-rtcompat', 'tokio-tungstenite', 'tungstenite', 'async-std', 'smol', 'tauri-plugin-http',
  'tauri-plugin-websocket', 'tauri-plugin-upload', 'tauri-plugin-shell']

const crateDirs = () => fs.readdirSync(APP, { withFileTypes: true })
  .filter(e => e.isDirectory() && fs.existsSync(path.join(APP, e.name, 'Cargo.toml'))).map(e => e.name)

test('solo il crate transport dichiara librerie di rete', () => {
  const dirs = crateDirs()
  assert.ok(dirs.includes('transport') && dirs.includes('shell'), `crate trovati: ${dirs}`)
  for (const d of dirs.filter(d => d !== 'transport')) {
    const toml = read(path.join(APP, d, 'Cargo.toml'))
    for (const c of NET_CRATES) assert.ok(!new RegExp(`^\\s*${c}\\s*=`, 'm').test(toml), `app/${d}: dipendenza di rete ${c}`)
    // tokio con socket solo nel trasporto
    const tk = /^\s*tokio\s*=\s*(.*)$/m.exec(toml)
    assert.ok(!tk || !/"net"|"full"/.test(tk[1]), `app/${d}: tokio con funzioni di rete`)
  }
})

test('nessun socket o client HTTP nel codice Rust fuori dal trasporto', () => {
  const BAD = [/std::net\b/, /tokio::net\b/, /\bTcpStream\b/, /\bTcpListener\b/, /\bUdpSocket\b/, /\breqwest::/, /\bhyper::/, /\bureq::/, /\bsocket2::/]
  for (const d of crateDirs().filter(d => d !== 'transport')) {
    for (const f of walk(path.join(APP, d)).filter(f => f.endsWith('.rs'))) {
      const src = read(f)
      for (const re of BAD) assert.ok(!re.test(src), `${path.relative(ROOT, f)}: ${re}`)
    }
  }
})

test("l'interfaccia non fa richieste di rete", () => {
  const BAD = [/\bfetch\s*\(/, /\bWebSocket\b/, /\bXMLHttpRequest\b/, /\bEventSource\b/, /sendBeacon/, /\bimport\s*\(/,
    /(?:src|href|action)\s*=\s*["']?\s*(?:https?:|wss?:|\/\/)/i, /url\(\s*["']?\s*(?:https?:|\/\/)/i, /@import/i]
  const files = walk(path.join(APP, 'ui'))
  assert.ok(files.length > 0)
  for (const f of files) {
    const src = read(f)
    for (const re of BAD) assert.ok(!re.test(src), `${path.relative(ROOT, f)}: ${re}`)
  }
})

test('connect_onion accetta solo un OnionV3 validato', () => {
  const lib = read(path.join(APP, 'transport', 'src', 'lib.rs'))
  const sig = /fn connect_onion\(\s*&self,\s*host:\s*&OnionV3,/.exec(lib)
  assert.ok(sig, 'la firma di connect_onion non prende &OnionV3')
  // nessun altro metodo pubblico del trait che prenda un host come testo
  const tr = /pub trait Transport[\s\S]*?\n}/.exec(lib)
  assert.ok(tr)
  assert.ok(!/:\s*&str|:\s*String|SocketAddr|IpAddr/.test(tr[0]), 'il trait Transport accetta host in chiaro')
  // OnionV3 non ha campi pubblici: si costruisce solo da parse() o da una chiave
  const onion = read(path.join(APP, 'transport', 'src', 'onion.rs'))
  assert.ok(/pub struct OnionV3\s*\{\s*pubkey:/.test(onion), 'OnionV3 con campi pubblici')
})
