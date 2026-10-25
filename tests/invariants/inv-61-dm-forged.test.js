// @ts-check
'use strict'
/**
 * I-61 — Una busta forgiata o malformata viene scartata in silenzio, mai con un panico e mai con un messaggio
 * mostrato: sigillo di un altro, firma del sigillo falsa, messaggio firmato, kind sbagliati, versione NIP-44
 * sconosciuta, MAC alterato, destinatario sbagliato, ora nel futuro, rete sbagliata, contenuto tagliato.
 * Dentro un messaggio aperto non c'è nient'altro che testo e data.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const path = require('path')
const { spawnSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..', '..')
const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })

test('ogni busta forgiata è scartata; la buona passa', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const r = spawnSync('cargo', ['test', '--manifest-path', path.join(ROOT, 'app', 'Cargo.toml'), '-p', 'm4tr1x-dm', '--test', 'inv_61_forgiate'],
    { cwd: path.join(ROOT, 'app'), encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  for (const t of ['ogni_difetto_e_scartato_e_la_busta_buona_passa', 'rete_sbagliata_contenuto_tagliato_e_forme_strane_scartati_senza_panico', 'si_apre_solo_con_una_chiave_dm', 'dentro_non_ci_sono_chiavi_ne_dati_che_il_messaggio_non_ha_scritto'])
    assert.match(out, new RegExp(`${t} \\.\\.\\. ok`), `manca o fallisce la prova ${t}`)
})
