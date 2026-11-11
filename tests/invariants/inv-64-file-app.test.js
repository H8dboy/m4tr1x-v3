// @ts-check
'use strict'
/**
 * I-64 — Reels, film, musica nell'app:
 *  (1) un file parte SOLO dopo la pulizia sul computer (I-16): `media_prepara` pulisce, il guscio tiene SOLO il file
 *      pulito, `media_pubblica` carica quello (mai il nome né il percorso: dall'interfaccia arrivano solo i byte);
 *  (2) un file scaricato si accetta SOLO se l'hash è quello della scheda (I-17) e i primi byte sono del tipo che la
 *      scheda dichiara; il client dei file legge solo risposte con Content-Length, entro un massimo e un tempo;
 *  (3) le schede si leggono solo nella forma giusta (un hash, tipo da un elenco chiuso e adatto alla sezione, una
 *      sezione, dimensione entro il massimo), anche se il nodo manda di tutto;
 *  (4) l'interfaccia mostra i file solo da indirizzi blob: (CSP: blob: SOLO per immagini e audio/video, con I-01).
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const { spawnSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..', '..')
const APP = path.join(ROOT, 'app')
const read = (/** @type {string} */ f) => fs.readFileSync(f, 'utf8').replace(/\/\/[^\n]*/g, '')
const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })
const corpo = (/** @type {string} */ src, /** @type {string} */ nome) => {
  const m = new RegExp(`fn ${nome}\\b[\\s\\S]*?\\n}\\n`).exec(src)
  assert.ok(m, `funzione ${nome} non trovata`)
  return m[0]
}

test('guscio: si pulisce prima di tutto, si tiene e si carica solo il file pulito, si scarica con hash e tipo', () => {
  const lib = read(path.join(APP, 'shell', 'src', 'lib.rs'))
  const prepara = corpo(lib, 'media_prepara')
  assert.match(prepara, /m4tr1x_media::ripulisci\(b\)/, 'media_prepara non pulisce il file')
  assert.match(prepara, /Some\(\(pulito, mime\)\)/, 'media_prepara tiene altro oltre al file pulito')
  assert.ok(!/Some\(\(b\.|to_vec\(\), mime\)\)/.test(prepara), 'media_prepara tiene il file originale')
  const pubblica = corpo(lib, 'media_pubblica')
  assert.match(pubblica, /scelto\.0\.lock\(\)/, 'media_pubblica non usa il file preparato')
  assert.match(pubblica, /carica_file\(&\*tor, &nodo, &intestazione, &file,/, 'media_pubblica carica un file diverso da quello preparato')
  assert.ok(!/std::fs|PathBuf|read_to/.test(prepara + pubblica), 'si legge un file dal disco: deve arrivare solo il contenuto scelto')
  const scarica = corpo(lib, 'media_file')
  assert.match(scarica, /if !tipo_coerente\(&b, &mime\)/, 'media_file non controlla il tipo del file scaricato')
  const http = read(path.join(APP, 'transport', 'src', 'http.rs'))
  assert.match(corpo(http, 'scarica'), /if sha256_hex\(&r\.corpo\) != sha256 \{/, 'il client dei file non controlla l\'hash')
  const conf = JSON.parse(fs.readFileSync(path.join(APP, 'shell', 'tauri.conf.json'), 'utf8')).app.security.csp
  assert.equal(conf['img-src'], "'self' blob:")
  assert.equal(conf['media-src'], "'self' blob:")
  for (const [d, v] of Object.entries(conf)) if (d !== 'img-src' && d !== 'media-src') assert.ok(!String(v).includes('blob:'), `blob: in ${d}`)
})

test('interfaccia: al lato Rust va solo il contenuto del file, mai il nome', () => {
  const js = read(path.join(APP, 'ui', 'main.js'))
  assert.match(js, /invoke\('media_prepara', new Uint8Array\(await f\.arrayBuffer\(\)\)\)/)
  assert.ok(!/f\.name|\.webkitRelativePath|\.path\b/.test(js), 'l\'interfaccia legge il nome o il percorso del file')
})

test('file (Rust): client HTTP del trasporto e schede della sezione', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  for (const [pacchetto, argomenti, prove] of /** @type {[string, string[], string[]][]} */ ([
    ['m4tr1x-transport', ['--lib', 'http::'], ['scarica_solo_il_file_con_l_hash_chiesto', 'risposte_strane_si_rifiutano', 'un_nodo_che_non_risponde_scade', 'carica_manda_l_autorizzazione_e_legge_il_motivo_di_un_no']],
    ['m4tr1x-feed', ['--test', 'media'], ['in_ogni_sezione_solo_schede_valide_dalla_piu_recente', 'schede_e_autorizzazioni_si_scrivono_solo_nella_forma_giusta']],
  ])) {
    const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(APP, 'Cargo.toml'), '-p', pacchetto, ...argomenti],
      { cwd: APP, encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
    const out = (r.stdout || '') + (r.stderr || '')
    assert.equal(r.status, 0, out.slice(-4000))
    for (const t of prove) assert.match(out, new RegExp(`${t} \\.\\.\\. ok`), `manca o fallisce la prova ${t}`)
  }
})
