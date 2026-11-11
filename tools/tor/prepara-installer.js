#!/usr/bin/env node
// @ts-check
'use strict'
/**
 * Prepara il tor da includere nell'installer dell'app (ADR-0016, I-37): scarica il bundle indicato dal lock
 * della piattaforma (solo https), controlla l'hash del bundle, ne estrae tor e le librerie del lock, ricontrolla
 * l'hash di ognuno e li mette in app/shell/tor-incluso/ (fuori da git). L'app, all'avvio, li confronta di nuovo
 * con il lock compilato dentro di lei prima di usarli.
 *
 * Uso: node tools/tor/prepara-installer.js        (poi: cargo tauri build --config app/shell/tauri.installer.conf.json)
 */
const fs = require('fs')
const os = require('os')
const path = require('path')
const crypto = require('crypto')
const https = require('https')
const { spawnSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..', '..')
const DEST = path.join(ROOT, 'app', 'shell', 'tor-incluso')
const LOCK = { win32: 'tor-windows.lock.json', linux: 'tor-linux.lock.json' }[/** @type {'win32'|'linux'} */ (process.platform)]

const sha256 = (/** @type {Buffer} */ b) => crypto.createHash('sha256').update(b).digest('hex')

/** Scarica seguendo al massimo 5 redirect, SOLO https. @param {string} url @returns {Promise<Buffer>} */
function scarica(url, salti = 0) {
  return new Promise((resolve, reject) => {
    if (!url.startsWith('https://')) return reject(new Error(`solo https: ${url}`))
    https.get(url, { timeout: 60000 }, r => {
      if (r.statusCode && r.statusCode >= 300 && r.statusCode < 400 && r.headers.location) {
        r.resume()
        if (salti >= 5) return reject(new Error('troppi redirect'))
        return resolve(scarica(new URL(r.headers.location, url).toString(), salti + 1))
      }
      if (r.statusCode !== 200) { r.resume(); return reject(new Error(`HTTP ${r.statusCode} per ${url}`)) }
      const parti = /** @type {Buffer[]} */ ([])
      r.on('data', c => parti.push(c))
      r.on('end', () => resolve(Buffer.concat(parti)))
      r.on('error', reject)
    }).on('error', reject).on('timeout', function () { this.destroy(new Error('tempo scaduto')) })
  })
}

async function main() {
  if (!LOCK) throw new Error(`nessun lock di tor per ${process.platform}`)
  const lock = JSON.parse(fs.readFileSync(path.join(ROOT, 'app', LOCK), 'utf8'))
  const bundle = await scarica(lock.bundle_url)
  if (sha256(bundle) !== lock.bundle_sha256) throw new Error(`hash del bundle sbagliato: ${sha256(bundle)}`)
  const lavoro = fs.mkdtempSync(path.join(os.tmpdir(), 'm4-tor-'))
  try {
    const tgz = path.join(lavoro, 'bundle.tar.gz')
    fs.writeFileSync(tgz, bundle)
    const dir = path.dirname(lock.tor_path_in_bundle)
    const voci = [lock.tor_path_in_bundle, ...Object.keys(lock.librerie || {}).map(l => `${dir}/${l}`)]
    // percorsi relativi dentro la cartella di lavoro: un tar GNU leggerebbe "C:\..." come un server remoto
    const r = spawnSync('tar', ['-xzf', 'bundle.tar.gz', ...voci], { cwd: lavoro, encoding: 'utf8' })
    if (r.status !== 0) throw new Error(`estrazione fallita: ${r.stderr}`)
    fs.rmSync(DEST, { recursive: true, force: true })
    fs.mkdirSync(DEST, { recursive: true })
    const attesi = /** @type {[string, string][]} */ ([[lock.tor_path_in_bundle, lock.tor_sha256], ...Object.entries(lock.librerie || {}).map(([l, h]) => [`${dir}/${l}`, h])])
    for (const [voce, hash] of attesi) {
      const b = fs.readFileSync(path.join(lavoro, voce))
      if (sha256(b) !== hash) throw new Error(`hash sbagliato per ${voce}`)
      fs.writeFileSync(path.join(DEST, path.basename(voce)), b, { mode: 0o755 })
    }
    console.log(`tor ${lock.tor_version} pronto per l'installer: ${attesi.length} file verificati in app/shell/tor-incluso/`)
  } finally {
    fs.rmSync(lavoro, { recursive: true, force: true })
  }
}

main().catch(e => { console.error(String(/** @type {Error} */ (e).message || e)); process.exit(1) })
