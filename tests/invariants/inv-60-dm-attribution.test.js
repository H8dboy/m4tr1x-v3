// @ts-check
'use strict'
/**
 * I-60 — Un messaggio privato è "di Anna" solo se la chiave DM che l'ha sigillato è annunciata da Anna:
 * annuncio (kind 10477) firmato dall'identità, con la prova di possesso della chiave DM (firma della chiave DM
 * sull'annuncio), il più recente vale, della rete giusta. Senza annuncio il mittente resta SCONOSCIUTO (mai
 * indovinato, mai preso dal contenuto del messaggio).
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const path = require('path')
const { spawnSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..', '..')
const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })

test('attribuzione del mittente solo con annuncio valido e prova di possesso', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const r = spawnSync('cargo', ['test', '--manifest-path', path.join(ROOT, 'app', 'Cargo.toml'), '-p', 'm4tr1x-dm', '--test', 'inv_60_attribuzione'],
    { cwd: path.join(ROOT, 'app'), encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  for (const t of ['l_annuncio_si_costruisce_solo_con_il_ruolo_giusto', 'l_annuncio_e_un_evento_firmato_dall_identita_con_la_prova_di_possesso', 'senza_annuncio_il_mittente_resta_sconosciuto',
    'vale_l_annuncio_piu_recente_della_stessa_identita', 'annuncio_di_un_altra_rete_o_non_valido_ignorato', 'con_l_annuncio_giusto_il_messaggio_e_di_anna',
    'prova_di_possesso_alterata_o_mancante_non_vale', 'un_impostore_non_si_prende_la_chiave_dm_di_un_altro',
    'se_due_identita_rivendicano_la_stessa_chiave_dm_nessuno_la_ottiene', 'un_evento_di_un_altro_kind_con_gli_stessi_tag_non_e_un_annuncio',
    // I-62: scadenza degli annunci (24/11/2026)
    'l_annuncio_scade_e_dopo_non_attribuisce_piu_niente', 'senza_scadenza_o_con_scadenza_storta_l_annuncio_non_vale', 'scaduto_il_piu_recente_non_torna_buono_uno_piu_vecchio_ancora_valido_ma_ruotato'])
    assert.match(out, new RegExp(`${t} \\.\\.\\. ok`), `manca o fallisce la prova ${t}`)
})
