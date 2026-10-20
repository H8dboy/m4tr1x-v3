#!/usr/bin/env node
// @ts-check
'use strict'
/**
 * Esegue i test file per file con le regole di tools/report/esito.js: un file che non
 * risponde (bloccato, uscito senza riepilogo, senza test) conta come FALLITO.
 * È lo stesso runner di `npm run report`: CI, `npm test` e report danno lo stesso esito.
 *
 * Uso: node tools/report/esegui.js [--senza-saltati] [file o cartella …]   (predefinito: tests)
 *   --senza-saltati  anche un test saltato fa fallire (job "tor" della CI: lì tor c'è,
 *                    una prova saltata vorrebbe dire che non ha girato)
 * Esce con 1 se c'è anche un solo fallito.
 */
const path = require('path')
const { eseguiFile, fileDiTest } = require('./esito')

const ROOT = path.resolve(__dirname, '..', '..')
const argv = process.argv.slice(2)
const senzaSaltati = argv.includes('--senza-saltati')
const voci = argv.filter(a => a !== '--senza-saltati')
const files = fileDiTest(ROOT, voci.length ? voci : ['tests'])
if (!files.length) { console.error('nessun file di test trovato'); process.exit(1) }

const tot = { pass: 0, fail: 0, skipped: 0, todo: 0 }
for (const rel of files) {
  const t = eseguiFile(rel, { cwd: ROOT })
  if (senzaSaltati && t.skipped) {
    t.problema = [t.problema, `${t.skipped} test saltati (${t.skipReasons.join('; ')})`].filter(Boolean).join('; ')
    t.fail += t.skipped
  }
  for (const k of /** @type {const} */ (['pass', 'fail', 'skipped', 'todo'])) tot[k] += t[k]
  const riga = `${t.fail ? 'FALLITO' : 'ok     '} ${rel} — ${t.pass} verdi, ${t.fail} falliti, ${t.skipped} saltati, ${t.todo} in attesa (${(t.ms / 1000).toFixed(1)} s)`
  console.log(riga)
  if (t.problema) console.log(`  ${t.problema}`)
  for (const d of t.diagnostiche) console.log(`  # ${d}`)
  // le ultime righe dei file falliti: in CI servono per capire senza rieseguire
  if (t.uscita) console.log(t.uscita)
}
console.log(`\nTotale: ${tot.pass} verdi, ${tot.fail} falliti, ${tot.skipped} saltati, ${tot.todo} in attesa · ${files.length} file`)
process.exit(tot.fail ? 1 : 0)
