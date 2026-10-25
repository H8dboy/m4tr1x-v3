// @ts-check
'use strict'
/**
 * I-20 — La busta è firmata da una chiave usa e getta e ha la data sfalsata (fino a 2 giorni indietro, mai nel futuro).
 * Dinamico (Rust, app/dm): pubkey della busta ≠ mittente e diversa a ogni invio; created_at nell'intervallo;
 * il messaggio è riempito (la lunghezza esatta non si vede).
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const path = require('path')
const { spawnSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..', '..')
const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })

test('chiave usa e getta, data sfalsata, messaggio riempito, vettori NIP-44 ufficiali', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const r = spawnSync('cargo', ['test', '--manifest-path', path.join(ROOT, 'app', 'Cargo.toml'), '-p', 'm4tr1x-dm', '--test', 'inv_19_20_buste', '--test', 'nip44'],
    { cwd: path.join(ROOT, 'app'), encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  for (const t of ['i20_ogni_busta_ha_una_chiave_usa_e_getta', 'i20_la_data_e_sfalsata_indietro_fino_a_due_giorni', 'il_messaggio_e_riempito_e_la_dimensione_non_rivela_la_lunghezza_esatta', 'cifra_e_decifra', 'chiave_di_conversazione', 'chiavi_del_messaggio', 'padding'])
    assert.match(out, new RegExp(`${t} \\.\\.\\. ok`), `manca o fallisce la prova ${t}`)
})
