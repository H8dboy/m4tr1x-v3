#!/usr/bin/env node
// @ts-check
'use strict'
/**
 * Controllo dei segreti nei file tracciati (invariante I-28).
 * Affianca gitleaks (che gira in CI): regole specifiche di M4TR1X che gitleaks
 * non conosce (chiavi Nostr, chiavi onion, file di dati, IP, email personali).
 *
 * Uso: node tools/security/scan-secrets.js      → controlla i file tracciati da git
 */
const fs = require('fs')
const path = require('path')
const { execFileSync } = require('child_process')

/** Nomi di file che non devono MAI essere tracciati. */
const FORBIDDEN_NAMES = [
  /(^|\/)\.env(\.|$)/, /\.db$/, /\.db-(wal|shm)$/, /\.sqlite3?$/,
  /hs_ed25519_secret_key$/, /hs_ed25519_public_key$/, /(^|\/)hostname$/,
  /\.(keystore|jks|p12|pfx|pem|key|enc|seed)$/, /(^|\/)node_config\.json$/,
  /\.(apk|aab|ipa|exe|dmg|msi|AppImage|deb)$/,
]

/** Contenuti vietati: [nome regola, regex]. */
const RULES = [
  ['chiave privata Nostr (nsec)', /\bnsec1[02-9ac-hj-np-z]{58}\b/],
  ['chiave privata PEM', /-----BEGIN (RSA |EC |OPENSSH |ENCRYPTED )?PRIVATE KEY-----/],
  ['chiave segreta minisign', /^untrusted comment: minisign encrypted secret key/m],
  ['indirizzo onion v3', /\b[a-z2-7]{56}\.onion\b/],
  ['IP privato', /\b(?:192\.168|10\.\d{1,3}|172\.(?:1[6-9]|2\d|3[01]))\.\d{1,3}\.\d{1,3}\b/],
  ['IP pubblico', /\b(?!127\.|0\.|192\.0\.2\.|198\.51\.100\.|203\.0\.113\.)(?:(?:25[0-5]|2[0-4]\d|1?\d?\d)\.){3}(?:25[0-5]|2[0-4]\d|1?\d?\d)\b/],
  ['email personale', /\b[A-Za-z0-9._%+-]+@(?!users\.noreply\.github\.com|example\.(?:com|org)|anthropic\.com)[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b/],
  ['chiave esadecimale da 64 byte', /\b[0-9a-f]{128}\b/],
]

/** File esclusi dal controllo dei contenuti (documentazione degli esempi di test). */
const CONTENT_ALLOWLIST = [/^tools\/security\/scan-secrets\.js$/, /^tests\/invariants\/inv-28-secrets\.test\.js$/]

/**
 * @param {string} relPath
 * @param {string} content
 * @returns {string[]} violazioni trovate
 */
function scanFile(relPath, content) {
  const p = relPath.replace(/\\/g, '/')
  const out = []
  for (const re of FORBIDDEN_NAMES) if (re.test(p)) out.push(`${p}: file vietato (${re})`)
  if (CONTENT_ALLOWLIST.some(re => re.test(p))) return out
  for (const [name, re] of RULES) {
    const m = content.match(/** @type {RegExp} */ (re))
    if (m) out.push(`${p}: ${name} → "${m[0].slice(0, 24)}…"`)
  }
  return out
}

/** @param {string} root */
function scanRepo(root) {
  const files = execFileSync('git', ['ls-files', '-z'], { cwd: root }).toString('utf8').split('\0').filter(Boolean)
  const out = []
  for (const f of files) {
    const abs = path.join(root, f)
    if (!fs.existsSync(abs)) continue
    const buf = fs.readFileSync(abs)
    const text = buf.includes(0) ? '' : buf.toString('utf8')   // binari: solo controllo del nome
    out.push(...scanFile(f, text))
  }
  return out
}

module.exports = { scanFile, scanRepo }

if (require.main === module) {
  const found = scanRepo(path.resolve(__dirname, '..', '..'))
  if (found.length) { console.error('SEGRETI O FILE VIETATI:\n  ' + found.join('\n  ')); process.exit(1) }
  console.log('nessun segreto trovato nei file tracciati')
}
