// @ts-check
'use strict'
/**
 * Profilo (kind 0, F4) — decisioni del fondatore del 20/10/2026: solo `name` e `about` (niente lud16/lud06,
 * nip05, picture: l'immagine arriva in F6), nome senza caratteri invisibili o di direzione (falsificazione
 * di nomi), limiti 50/500, e nel feed il nome viene SOLO da un profilo verificato, il più recente.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const path = require('path')
const { spawnSync } = require('child_process')

const APP = path.resolve(__dirname, '..', '..', 'app')
const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })

test('profilo (Rust): solo nome e descrizione, nomi ripuliti, nome nel feed solo da profili verificati', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const esegui = (/** @type {string[]} */ a) => {
    const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(APP, 'Cargo.toml'), ...a], { cwd: APP, encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
    const out = (r.stdout || '') + (r.stderr || '')
    assert.equal(r.status, 0, out.slice(-4000))
    return out
  }
  const protocollo = esegui(['-p', 'm4tr1x-protocol', '--test', 'profilo'])
  for (const p of ['solo_name_e_about_il_resto_si_scarta', 'nomi_falsificati_ripuliti', 'testi_lunghi_tagliati_in_lettura',
    'contenuti_non_validi_ignorati', 'scrittura_solo_due_campi_e_limiti', 'profilo_con_dati_h8_non_si_firma'])
    assert.match(protocollo, new RegExp(`${p} \\.\\.\\. ok`), `manca o fallisce la prova ${p}`)
  const feed = esegui(['-p', 'm4tr1x-feed', '--test', 'feed'])
  for (const p of ['il_feed_mostra_il_nome_dal_profilo_piu_recente', 'leggi_profilo_per_precompilare', 'profilo_salvato_si_rilegge_e_da_il_nome'])
    assert.match(feed, new RegExp(`${p} \\.\\.\\. ok`), `manca o fallisce la prova ${p}`)
})
