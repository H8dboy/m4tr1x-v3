// @ts-check
'use strict'
/**
 * Riscaldamento della cache di tor (CI e macchine di sviluppo; decisione del fondatore,
 * 16/10/2026). Avvia tor SENZA servizi onion, aspetta il bootstrap completo e salva nella
 * cache condivisa dei test solo i file pubblici e firmati (`cached-*`), mai chiavi né `state`.
 * Dopo, i test con tor vero partono a caldo: un rosso dice "il nostro codice è rotto",
 * non "la rete Tor era lenta a scaricare il consenso".
 *
 *   M4TR1X_TOR_BIN=/usr/bin/tor node tools/tor/riscalda.js
 */
const fs = require('fs')
const os = require('os')
const path = require('path')
const { spawn } = require('child_process')
const { TOR_CACHE_DIR, TOR_CACHE_FILES } = require('../../tests/helpers/node')

const bin = process.env.M4TR1X_TOR_BIN
if (!bin || !fs.existsSync(bin)) { console.error('M4TR1X_TOR_BIN non impostato'); process.exit(2) }
const MAX_MS = Number(process.env.M4TR1X_RISCALDA_MAX_SEC || 600) * 1000

const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'm4-riscalda-'))
fs.chmodSync(dir, 0o700)
const t0 = Date.now()
const tor = spawn(bin, ['--DataDirectory', dir, '--SocksPort', '0', '--Log', 'notice stdout', '--ignore-missing-torrc', '-f', path.join(dir, 'nessun-torrc')], { stdio: ['ignore', 'pipe', 'ignore'] })
const scadenza = setTimeout(() => { console.error(`riscaldamento: bootstrap non completato in ${MAX_MS / 1000} s`); tor.kill(); process.exit(1) }, MAX_MS)
let ultimo = ''
tor.stdout.on('data', d => {
  const m = /Bootstrapped (\d+)%/.exec(d.toString())
  if (m && m[1] !== ultimo) { ultimo = m[1]; console.log(`riscaldamento: ${m[1]}%`) }
  if (/Bootstrapped 100%/.test(d.toString())) {
    clearTimeout(scadenza)
    tor.kill()
    tor.once('exit', () => {
      fs.mkdirSync(TOR_CACHE_DIR, { recursive: true, mode: 0o700 })
      const copiati = TOR_CACHE_FILES.filter(f => {
        try { fs.copyFileSync(path.join(dir, f), path.join(TOR_CACHE_DIR, f)); return true } catch { return false }
      })
      fs.rmSync(dir, { recursive: true, force: true })
      console.log(`riscaldamento: completato in ${((Date.now() - t0) / 1000).toFixed(1)} s, in cache: ${copiati.join(', ') || 'niente'}`)
      process.exit(copiati.length ? 0 : 1)
    })
  }
})
