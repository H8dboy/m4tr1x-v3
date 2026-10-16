// @ts-check
'use strict'
/**
 * I-58 — Finestra del seme (decisione del fondatore del 16/10/2026, opzione A).
 * Le 24 parole compaiono SOLO nella finestra del seme, mai salvate; la finestra
 * principale non le riceve e non può chiamare i comandi del seme. Dopo la creazione
 * l'identità si salva solo se l'utente conferma 3 parole a caso (posizioni scelte dal
 * lato Rust). Nella finestra del seme copia, incolla, taglia, trascina e menu contestuale
 * sono disattivati; su Windows la finestra è esclusa dalle catture dello schermo
 * (SetWindowDisplayAffinity con WDA_EXCLUDEFROMCAPTURE) PRIMA di essere mostrata.
 * Nel portachiavi del sistema finiscono solo identity, dm e wallet: mai seme, parole, revoca.
 *
 * Parte statica qui; la parte dinamica (conferma sbagliata → niente salvato, revoca mai
 * salvata, parole non nel portachiavi) è nei test Rust di app/identity, lanciati da qui.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const { spawnSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..', '..')
const SHELL = path.join(ROOT, 'app', 'shell')
const UI = path.join(ROOT, 'app', 'ui')
const read = (/** @type {string} */ p) => fs.readFileSync(p, 'utf8')
const exists = (/** @type {string} */ p) => fs.existsSync(p)

const SEED_CMDS = ['seme_nuovo', 'seme_conferma', 'seme_recupera']

function capability(/** @type {string} */ id) {
  for (const f of fs.readdirSync(path.join(SHELL, 'capabilities'))) {
    const c = JSON.parse(read(path.join(SHELL, 'capabilities', f)))
    if (c.identifier === id) return c
  }
  return null
}

test('i comandi del seme esistono e li può chiamare SOLO la finestra del seme', () => {
  const build = read(path.join(SHELL, 'build.rs'))
  for (const c of SEED_CMDS) assert.ok(build.includes(`"${c}"`), `manca il comando ${c} in build.rs`)
  const seme = capability('seme')
  assert.ok(seme, 'manca la capability "seme"')
  assert.deepEqual(seme.windows, ['seme'], 'la capability del seme vale per altre finestre')
  assert.deepEqual([...seme.permissions].sort(), SEED_CMDS.map(c => `allow-${c.replace(/_/g, '-')}`).sort(), 'la finestra del seme ha permessi in più')
  for (const f of fs.readdirSync(path.join(SHELL, 'capabilities'))) {
    const c = JSON.parse(read(path.join(SHELL, 'capabilities', f)))
    if (c.identifier === 'seme') continue
    for (const p of c.permissions) assert.ok(!/allow-seme-/.test(String(p)), `${f}: la finestra ${c.windows} può chiamare ${p}`)
  }
  // seconda difesa nel lato Rust: ogni comando seme_* controlla l'etichetta della finestra
  const lib = read(path.join(SHELL, 'src', 'lib.rs'))
  assert.match(lib, /fn solo_finestra_seme\(w: &WebviewWindow\) -> Result<\(\), String> \{\s*if w\.label\(\) == FINESTRA_SEME \{/, 'solo_finestra_seme non confronta l\'etichetta')
  for (const c of SEED_CMDS) {
    const body = new RegExp(`fn ${c}\\([^)]*\\)[^{]*\\{([\\s\\S]*?)\\n\\}`).exec(lib)
    assert.ok(body, `manca la funzione ${c}`)
    assert.match(body[1].trim().split('\n')[0], /^solo_finestra_seme\(&window\)\?;$/, `${c} non controlla per prima cosa la finestra`)
  }
})

test("la finestra principale non tocca il seme", () => {
  for (const f of ['index.html', 'main.js']) {
    const src = read(path.join(UI, f))
    for (const c of SEED_CMDS) assert.ok(!src.includes(c), `${f} chiama ${c}`)
    assert.ok(!/seme\.js/.test(src), `${f} carica seme.js`)
  }
})

test('pagina del seme: solo il suo script, niente copia/incolla/menu, testo non selezionabile', () => {
  assert.ok(exists(path.join(UI, 'seme.html')) && exists(path.join(UI, 'seme.js')), 'manca la pagina del seme')
  const html = read(path.join(UI, 'seme.html'))
  const scripts = [...html.matchAll(/<script[^>]*src="([^"]+)"/g)].map(m => m[1])
  assert.deepEqual(scripts, ['seme.js'], 'la pagina del seme carica altri script')
  const js = read(path.join(UI, 'seme.js'))
  for (const ev of ['copy', 'cut', 'paste', 'contextmenu', 'dragstart', 'drop', 'selectstart'])
    assert.ok(new RegExp(`['"]${ev}['"]`).test(js), `seme.js non blocca l'evento ${ev}`)
  assert.ok(/preventDefault\(\)/.test(js), 'seme.js non annulla gli eventi')
  assert.ok(!/localStorage|sessionStorage|indexedDB|document\.cookie/.test(js), 'seme.js salva qualcosa nel browser')
  assert.ok(!/console\.(log|info|debug)/.test(js), 'seme.js stampa nella console')
  const css = read(path.join(UI, 'seme.css'))
  assert.ok(/user-select:\s*none/.test(css), 'parole selezionabili')
})

test('finestra del seme esclusa dalle catture su Windows prima di essere mostrata', () => {
  const src = fs.readdirSync(path.join(SHELL, 'src')).map(f => read(path.join(SHELL, 'src', f))).join('\n')
  assert.ok(/SetWindowDisplayAffinity/.test(src) && /WDA_EXCLUDEFROMCAPTURE/.test(src), 'manca il blocco delle catture')
  const fn = /fn apri_finestra_seme[\s\S]*?\n}/.exec(src)
  assert.ok(fn, 'manca apri_finestra_seme')
  assert.ok(/\.visible\(false\)/.test(fn[0]), 'la finestra del seme nasce visibile (prima del blocco)')
  const iAff = fn[0].indexOf('escludi_dalle_catture'), iShow = fn[0].indexOf('.show()')
  assert.ok(iAff > 0 && iShow > iAff, 'la finestra viene mostrata prima di escluderla dalle catture')
  const etichetta = /"seme"/.test(fn[0]) || (/FINESTRA_SEME/.test(fn[0]) && /const FINESTRA_SEME: &str = "seme";/.test(src))
  assert.ok(etichetta, 'finestra del seme senza etichetta "seme"')
})

const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })
test('parte dinamica (Rust): conferma di 3 parole, revoca e parole mai nel portachiavi', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const r = spawnSync('cargo', ['test', '--manifest-path', path.join(ROOT, 'app', 'Cargo.toml'), '-p', 'm4tr1x-identity', '--', 'creazione::'],
    { cwd: path.join(ROOT, 'app'), encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  for (const t of ['conferma_sbagliata_non_salva_niente', 'conferma_giusta_salva_solo_tre_ruoli', 'tre_posizioni_diverse_e_casuali', 'recupero_salva_le_stesse_chiavi'])
    assert.match(out, new RegExp(`${t} \\.\\.\\. ok`), `manca o fallisce la prova ${t}`)
})
