// @ts-check
'use strict'
/**
 * I-37 — Il Tor di Windows serve solo onion e non presta i circuiti dell'app
 * (corretta il 16/10/2026, ADR-0016). Le prove sono in Rust, nel crate del trasporto
 * (app/transport/tests/inv_37_tor_process.rs); qui si lanciano con cargo, così la
 * regola compare nel report e nelle prove di sabotaggio come le altre.
 *
 * Senza M4TR1X_TOR_BIN girano solo le prove senza tor (hash sbagliato → non parte);
 * con M4TR1X_TOR_BIN (job "tor" della CI) anche: niente host in chiaro senza
 * credenziali, cookie obbligatorio sul controllo, niente fuori da 127.0.0.1,
 * tor muore con l'app, e un servizio onion di prova raggiunto davvero.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const path = require('path')
const fs = require('fs')
const { spawnSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..', '..')
const MANIFEST = path.join(ROOT, 'app', 'Cargo.toml')
const TOR = process.env.M4TR1X_TOR_BIN
const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })
const noCargo = cargo.status !== 0 ? 'cargo non disponibile: prove Rust di I-37 saltate' : false

/** cargo test sul crate del trasporto; `target` = argomenti di selezione, `filter` = nome della prova. */
function cargoTest(/** @type {string[]} */ target, filter = '') {
  const args = ['test', '--manifest-path', MANIFEST, '-p', 'm4tr1x-transport', ...target, '--', '--nocapture', '--test-threads=4']
  if (filter) args.push(filter)
  const r = spawnSync('cargo', args,
    { cwd: path.join(ROOT, 'app'), encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, env: process.env, shell: process.platform === 'win32' })
  return { code: r.status, out: (r.stdout || '') + (r.stderr || '') }
}

test('I-37 senza tor: binario con hash sbagliato non parte; porta di controllo solo da 127.0.0.1', { skip: noCargo, timeout: 900000 }, () => {
  const unit = cargoTest(['--lib'], 'tor_process::')
  assert.equal(unit.code, 0, unit.out.slice(-4000))
  assert.match(unit.out, /file_della_porta_solo_loopback \.\.\. ok/)
  const hash = cargoTest(['--test', 'inv_37_tor_process'], 'binario_con_hash_sbagliato_non_parte')
  assert.equal(hash.code, 0, hash.out.slice(-4000))
  assert.match(hash.out, /binario_con_hash_sbagliato_non_parte \.\.\. ok/)
})

test('I-37 con tor vero: solo onion, cookie, solo 127.0.0.1, muore con l\'app, onion raggiunto', {
  skip: noCargo || (!TOR || !fs.existsSync(TOR) ? 'M4TR1X_TOR_BIN non impostato: prove con tor vero saltate' : false),
  timeout: 900000,
}, () => {
  const r = cargoTest(['--test', 'inv_37_tor_process'])
  assert.equal(r.code, 0, r.out.slice(-4000))
  assert.ok(!r.out.includes('SALTATO: serve M4TR1X_TOR_BIN'), 'le prove Rust si sono saltate anche se M4TR1X_TOR_BIN è impostato')
  // 4 prove + `aiuto_fa_l_app` (che senza la sua variabile non fa niente e risulta passata)
  assert.match(r.out, /test result: ok\. 5 passed/, 'attese le 4 prove di I-37 (più l\'aiuto) passate')
})
