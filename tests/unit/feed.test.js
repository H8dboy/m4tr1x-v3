// @ts-check
'use strict'
/**
 * Feed dell'app (app/feed, F4 passo 4) con un trasporto finto: solo post verificati in Rust arrivano
 * all'interfaccia; tentativi da 30 s fino al budget di 4 minuti, poi un minuto di attesa e una ripresa,
 * con la fase sempre segnalata (decisioni del fondatore, 20/10/2026). La prova con un nodo vero via Tor
 * è tests/integration/feed-via-tor.test.js.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const path = require('path')
const { spawnSync } = require('child_process')

const APP = path.resolve(__dirname, '..', '..', 'app')
const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })

test('feed (Rust): post verificati, tentativi, budget, ripresa', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(APP, 'Cargo.toml'), '-p', 'm4tr1x-feed'],
    { cwd: APP, encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  for (const p of ['solo_post_verificati_dal_piu_recente', 'nessun_testo_di_eventi_non_validi_arriva_al_feed', 'riprova_finche_il_nodo_risponde',
    'oltre_il_budget_non_raggiungibile', 'dopo_il_budget_attende_un_minuto_e_riprova_da_sola', 'la_ripresa_e_una_sola'])
    assert.match(out, new RegExp(`${p} \\.\\.\\. ok`), `manca o fallisce la prova ${p}`)
})
