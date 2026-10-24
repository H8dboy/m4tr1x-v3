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
const { spawnSync } = require('child_process')

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

// Client del relay (app/transport/src/relay.rs, F4): il WebSocket lavora SOLO sul flusso onion che
// riceve. La libreria è compilata senza `connect` (aprirebbe socket TCP propri) e senza TLS.
test('client del relay: tokio-tungstenite senza connect né TLS', () => {
  const toml = read(path.join(APP, 'transport', 'Cargo.toml'))
  const riga = /^\s*tokio-tungstenite\s*=\s*(.*)$/m.exec(toml)
  assert.ok(riga, 'tokio-tungstenite non dichiarato nel trasporto')
  assert.match(riga[1], /default-features\s*=\s*false/, 'tokio-tungstenite con le funzioni predefinite (connect)')
  const f = /features\s*=\s*\[([^\]]*)\]/.exec(riga[1])
  const funzioni = f ? f[1].split(',').map(s => s.trim().replace(/"/g, '')).filter(Boolean) : []
  assert.deepEqual(funzioni, ['handshake'], `funzioni ammesse: solo handshake (trovate: ${funzioni})`)
  const rs = read(path.join(APP, 'transport', 'src', 'relay.rs'))
  assert.ok(!/connect_async|TcpStream|tokio::net/.test(rs), 'relay.rs apre connessioni proprie')
  assert.match(rs, /pub async fn apri\(flusso: Stream, host: &OnionV3\)/, 'Relay::apri deve prendere un flusso già aperto e un OnionV3')
})

const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })

// I-36 (runtime): OnionV3 rifiuta gli host in chiaro. Aggancia i test unitari di onion.rs al report (24/10/2026).
test('OnionV3 (Rust): rifiuta a runtime gli host in chiaro', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(APP, 'Cargo.toml'), '-p', 'm4tr1x-transport', '--lib', '--', 'onion::'],
    { cwd: APP, encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  for (const t of ['rifiuta_host_in_chiaro', 'rifiuta_onion_v2_e_lunghezze_sbagliate', 'rifiuta_checksum_sbagliato', 'rifiuta_versione_diversa_da_3', 'rifiuta_sottodomini_e_caratteri_non_base32'])
    assert.match(out, new RegExp(`${t} \\.\\.\\. ok`), `manca o fallisce la prova ${t}`)
})
test('client del relay (Rust): limiti di tempo, eventi e dimensione contro un relay finto', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(APP, 'Cargo.toml'), '-p', 'm4tr1x-transport', '--test', 'relay'],
    { cwd: APP, encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  for (const t of ['richiesta_eventi_fino_a_eose_poi_close', 'troppi_eventi_e_un_errore', 'messaggio_troppo_grande_e_un_errore', 'nodo_muto_tempo_massimo',
    'pubblica_aspetta_l_ok_del_proprio_evento', 'il_no_del_nodo_e_una_risposta_non_un_errore', 'pubblica_con_nodo_muto_tempo_massimo'])
    assert.match(out, new RegExp(`${t} \\.\\.\\. ok`), `manca o fallisce la prova ${t}`)
})
