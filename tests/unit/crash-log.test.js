// @ts-check
'use strict'
/**
 * Registro dei crash dell'app (app/crash, decisione del fondatore 20/10/2026): se l'app si richiude
 * da sola, crash.log nella cartella dell'app dice dove. Senza dati dell'utente: i messaggi costruiti
 * a runtime non si scrivono (prova Rust con un seme, un onion e un percorso finti).
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const { spawnSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..', '..')
const APP = path.join(ROOT, 'app')
const read = (/** @type {string} */ p) => fs.readFileSync(p, 'utf8')

test('il guscio installa il registro come PRIMA cosa, nella cartella dati dell\'app', () => {
  const lib = read(path.join(APP, 'shell', 'src', 'lib.rs'))
  const conf = JSON.parse(read(path.join(APP, 'shell', 'tauri.conf.json')))
  const id = /const IDENTIFICATORE: &str = "([^"]+)"/.exec(lib)
  assert.ok(id, 'IDENTIFICATORE non trovato')
  assert.equal(id[1], conf.identifier, 'la cartella del registro non è quella dell\'app')
  const run = /pub fn run\(\) \{([\s\S]*?)tauri::Builder::default\(\)/.exec(lib)
  assert.ok(run, 'run() non trovato')
  assert.match(run[1], /m4tr1x_crash::installa\(/, 'il registro dei crash non è installato prima di Tauri')
  // dal 21/10/2026 il riallineamento è il passo `prima` di costruisci (un solo setup: un secondo lo sostituirebbe)
  assert.match(lib, /costruisci\(tauri::Builder::default\(\), Arc::new\(Sistema\), move \|app\| \{\s*\/\/[^\n]*\n\s*let dati = app\.path\(\)\.app_data_dir\(\)\?;\s*if [^{]*\{\s*m4tr1x_crash::installa\(dati/, 'il registro non segue la cartella dati di Tauri')
  // ...e costruisci lo esegue per primo dentro il suo unico setup
  assert.match(lib, /\.setup\(move \|app\| \{\s*prima\(app\)\?;/, 'costruisci non esegue per primo il passo `prima` (registro dei crash)')
  assert.equal([...lib.matchAll(/\.setup\(/g)].length, 1, 'più di un .setup(: in Tauri l\'ultimo sostituisce gli altri')
})

const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })
test('registro dei crash (Rust): punto del codice sì, dati a runtime no', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(APP, 'Cargo.toml'), '-p', 'm4tr1x-crash'],
    { cwd: APP, encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  assert.match(out, /panic_registrati_senza_segreti \.\.\. ok/)
})
