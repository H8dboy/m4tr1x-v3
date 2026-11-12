// @ts-check
'use strict'
/**
 * I-16 — Nessun metadato identificativo lascia il dispositivo (GPS, modello, numero di serie, autore, programma,
 * date, commenti, volti riconosciuti). La pulizia sta in app/media e si fa PRIMA di caricare:
 *  - si accettano solo JPEG, PNG, MP4/MOV/M4A, MP3, riconosciuti dai primi byte; HEIC/AVIF rifiutati (la loro
 *    immagine sta proprio nel blocco che negli MP4 si azzera);
 *  - del JPEG resta solo l'immagine (più l'orientamento, riscritto da zero); del PNG solo i blocchi per disegnare;
 *  - negli MP4 restano solo le tracce video e audio: metadati, GPS, date azzerati a lunghezza invariata, e le tracce
 *    di metadati (iPhone: volti riconosciuti) tolte insieme ai loro campioni dentro i dati;
 *  - dell'MP3 resta solo l'audio (via ID3v2, ID3v1, APEv2);
 *  - un file rovinato si rifiuta, senza panico.
 * Parte dinamica: app/media/tests/pulizia.rs. Qui in più: la pulizia non ha dipendenze né rete.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const { spawnSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..', '..')
const APP = path.join(ROOT, 'app')
const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })

test('la pulizia non dipende da altro codice (né rete né librerie che decodificano)', () => {
  const toml = fs.readFileSync(path.join(APP, 'media', 'Cargo.toml'), 'utf8')
  const dip = /\[dependencies\]([\s\S]*?)(?:\n\[|$)/.exec(toml)
  assert.ok(dip)
  const nomi = [...dip[1].matchAll(/^\s*([a-z0-9_-]+)\s*=/gm)].map(m => m[1])
  assert.deepEqual(nomi, ['thiserror'], `dipendenze di m4tr1x-media: ${nomi}`)
})

test('pulizia (Rust): JPEG, PNG, MP4 con tracce di metadati, MP3, HEIC rifiutato, file rovinati', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(APP, 'Cargo.toml'), '-p', 'm4tr1x-media', '--test', 'pulizia'],
    { cwd: APP, encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  for (const t of ['jpeg_senza_metadati_con_l_orientamento_e_l_immagine_identica', 'jpeg_senza_exif_resta_senza_exif', 'png_senza_testo_date_ed_exif',
    'mp4_senza_gps_autore_modello_date_stessa_lunghezza_e_video_identico', 'mp4_le_tracce_di_metadati_spariscono_con_i_loro_campioni',
    'mp4_rovinati_si_rifiutano_senza_panico', 'mp3_senza_id3_ne_ape', 'solo_i_tipi_ammessi_e_entro_la_dimensione'])
    assert.match(out, new RegExp(`${t} \\.\\.\\. ok`), `manca o fallisce la prova ${t}`)
})
