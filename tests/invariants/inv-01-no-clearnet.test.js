// @ts-check
'use strict'
/**
 * I-01 — Nessuna richiesta di rete delle app esce fuori da Tor.
 *
 * Parte STATICA (attiva dal 21/10/2026, decisione del fondatore). Si appoggia a I-36 (solo il crate
 * `transport` apre connessioni, interfaccia senza richieste di rete) e a `cargo deny` in CI; qui ciò che
 * quelle prove non coprono:
 *  - CSP con elenco CHIUSO di sorgenti: I-31 vieta i token generici (https:, *), non un host esplicito;
 *  - i divieti di deny.toml non si tolgono: cargo deny passa anche senza il divieto se il crate oggi non c'è;
 *  - nessun plugin di Tauri (configurazione e dipendenze del guscio).
 * Parte DINAMICA (attiva dal 22/10/2026): job CI `i01-rete` (app vera, utente dedicato, solo relay Tor);
 * gli strumenti della prova in inv-01-rete.test.js. Su Windows: Wireshark a mano.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')

const ROOT = path.resolve(__dirname, '..', '..')
const APP = path.join(ROOT, 'app')
const read = (/** @type {string} */ p) => fs.readFileSync(p, 'utf8')

test('CSP: ogni sorgente di ogni direttiva è locale (elenco chiuso)', () => {
  const csp = JSON.parse(read(path.join(APP, 'shell', 'tauri.conf.json'))).app.security.csp
  const AMMESSE = new Set(["'self'", "'none'", 'ipc:', 'http://ipc.localhost'])
  assert.ok(Object.keys(csp).length > 0, 'CSP vuota')
  for (const [dir, val] of Object.entries(csp)) {
    for (const s of String(val).split(/\s+/).filter(Boolean)) assert.ok(AMMESSE.has(s), `CSP ${dir}: sorgente non locale ${s}`)
  }
  assert.equal(csp['default-src'], "'self'", 'default-src deve restare locale (vale per ogni direttiva non scritta)')
})

test('deny.toml: i divieti delle librerie di rete restano scritti', () => {
  const deny = read(path.join(APP, 'deny.toml'))
  const blocco = /\[bans\][\s\S]*?\ndeny = \[([\s\S]*?)\n\]/.exec(deny)
  assert.ok(blocco, 'elenco [bans] deny non trovato')
  const vietati = new Set([...blocco[1].matchAll(/^\s*\{ crate = "([\w-]+)"(?:, wrappers = \[([^\]]*)\])? \}/gm)].map(m => `${m[1]}${m[2] ? ` solo da ${m[2]}` : ''}`))
  for (const v of ['reqwest solo da "tauri"', 'hyper solo da "hyper-util", "reqwest"', 'hyper-util solo da "reqwest"',
    'tokio-tungstenite solo da "m4tr1x-transport"', 'tungstenite solo da "tokio-tungstenite"',
    'ureq', 'isahc', 'curl', 'surf', 'tauri-plugin-http', 'tauri-plugin-websocket', 'tauri-plugin-upload', 'tauri-plugin-shell',
    'tauri-plugin-updater', 'tauri-plugin-opener'])
    assert.ok(vietati.has(v), `deny.toml: manca o è cambiato il divieto "${v}"`)
})

test('nessun plugin di Tauri nel guscio', () => {
  const conf = JSON.parse(read(path.join(APP, 'shell', 'tauri.conf.json')))
  assert.deepEqual(Object.keys(conf.plugins || {}), [], 'plugin configurati in tauri.conf.json')
  const cargo = read(path.join(APP, 'shell', 'Cargo.toml'))
  assert.ok(!/^\s*tauri-plugin-/m.test(cargo), 'plugin di Tauri tra le dipendenze del guscio')
  const lib = read(path.join(APP, 'shell', 'src', 'lib.rs'))
  assert.ok(!/\.plugin\(/.test(lib), '.plugin( nel guscio')
})
