// @ts-check
'use strict'
/**
 * I-63 — Forum (stesso schema della v2: kind 1 con ["t","m4tr1x-forum"]):
 *  (1) è discussione solo un evento verificato del forum SENZA tag `e`; è risposta solo un evento del forum con
 *      UN solo tag `e` valido, e conta solo per quella discussione;
 *  (2) eventi non validi, di altri kind, risposte e post normali non diventano discussioni, anche se il nodo
 *      manda tutto (relay malevolo); il feed di Signal non mostra il forum;
 *  (3) titoli puliti come i nomi (niente caratteri invisibili o di direzione), con limite; nomi solo da profili
 *      verificati; si firma con la tua identità SOLO dal lato Rust (con I-31).
 * Parte dinamica: app/feed/tests/forum.rs (nodo finto e nodo malevolo).
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const path = require('path')
const { spawnSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..', '..')
const APP = path.join(ROOT, 'app')
const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })

test('forum (Rust): schema, nodo malevolo, feed separato, titoli puliti', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(APP, 'Cargo.toml'), '-p', 'm4tr1x-feed', '--test', 'forum'],
    { cwd: APP, encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  for (const t of ['il_forum_mostra_le_discussioni_dalla_piu_attiva_con_risposte_e_nomi', 'una_discussione_aperta_ha_solo_le_sue_risposte_in_ordine',
    'un_nodo_che_manda_di_tutto_non_confonde_forum_e_signal', 'una_risposta_non_si_apre_come_discussione', 'titoli_e_risposte_si_scrivono_solo_nella_forma_giusta'])
    assert.match(out, new RegExp(`${t} \\.\\.\\. ok`), `manca o fallisce la prova ${t}`)
})
