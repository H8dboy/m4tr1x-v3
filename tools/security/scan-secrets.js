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

/**
 * Indirizzi riservati alla documentazione (RFC 5737): si usano negli esempi e nei
 * test, non identificano nessuno, quindi sono ammessi. Per un IP d'esempio nei
 * test si usano QUESTI, mai IP veri spezzati per sfuggire al controllo.
 */
const DOC_IP_PREFIXES = Object.freeze(['192.0.2.', '198.51.100.', '203.0.113.'])

/**
 * Eccezione stretta (decisione del fondatore, 21/10/2026): un numero a quattro parti in un file .xml è
 * ammesso SOLO se è l'intero valore di un attributo `version` (es. version="6.0.0.0" nei manifest di
 * Windows). Stesse virgolette prima e dopo, `version=` subito prima; qualsiasi altra forma resta vietata.
 * @param {string} p @param {string} content @param {RegExpMatchArray} m
 */
function versioneXml(p, content, m) {
  if (!p.endsWith('.xml') || m.index === undefined) return false
  const i = m.index, q = content[i - 1]
  const a = i - 1 - 'version='.length   // inizio di `version=`: prima solo spazio o inizio del file
  return (q === '"' || q === "'") && content[i + m[0].length] === q && a >= 0 && content.slice(a, i - 1) === 'version=' && (a === 0 || /\s/.test(content[a - 1]))
}

/** Contenuti vietati: [nome regola, regex]. */
const RULES = [
  ['chiave privata Nostr (nsec)', /\bnsec1[02-9ac-hj-np-z]{58}\b/],
  ['chiave privata PEM', /-----BEGIN (RSA |EC |OPENSSH |ENCRYPTED )?PRIVATE KEY-----/],
  ['chiave segreta minisign', /^untrusted comment: minisign encrypted secret key/m],
  ['indirizzo onion v3', /\b[a-z2-7]{56}\.onion\b/],
  ['IP privato', /\b(?:192\.168|10\.\d{1,3}|172\.(?:1[6-9]|2\d|3[01]))\.\d{1,3}\.\d{1,3}\b/],
  // loopback e 0.0.0.0 ammessi; documentazione esclusa tramite DOC_IP_PREFIXES (sotto)
  ['IP pubblico', /\b(?!127\.|0\.)(?:(?:25[0-5]|2[0-4]\d|1?\d?\d)\.){3}(?:25[0-5]|2[0-4]\d|1?\d?\d)\b/g],
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
    const rx = /** @type {RegExp} */ (re)
    const hits = rx.global ? [...content.matchAll(rx)].map(m => m[0]) : (content.match(rx) || []).slice(0, 1)
    const real = name === 'IP pubblico'
      ? [...content.matchAll(rx)].filter(m => !DOC_IP_PREFIXES.some(d => m[0].startsWith(d)) && !versioneXml(p, content, m)).map(m => m[0])
      : hits
    if (real.length) out.push(`${p}: ${name} → "${real[0].slice(0, 24)}…"`)
  }
  return out
}

/**
 * Il testo con ogni contenuto vietato sostituito da `[oscurato: <regola>]`: stesse regole e stesse
 * eccezioni di scanFile (21/10/2026). Serve a chi scrive nel repo testo che non controlla, come il report
 * che copia l'uscita dei test falliti: senza, un rosso dello scanner finiva in report.json e il repo
 * restava rosso anche dopo aver corretto la causa. Proprietà (provata in inv-28): scanFile non trova più
 * niente nel contenuto, e un testo pulito resta identico.
 * @param {string} relPath @param {string} content
 */
function oscura(relPath, content) {
  const p = relPath.replace(/\\/g, '/')
  if (CONTENT_ALLOWLIST.some(re => re.test(p))) return content   // come scanFile
  let out = content
  for (const [name, re] of RULES) {
    const rx = /** @type {RegExp} */ (re)
    const g = new RegExp(rx.source, rx.flags.includes('g') ? rx.flags : rx.flags + 'g')
    out = out.replace(g, (/** @type {string} */ hit, /** @type {any[]} */ ...rest) => {
      if (name !== 'IP pubblico') return `[oscurato: ${name}]`
      const index = /** @type {number} */ (rest.at(-2)), testo = /** @type {string} */ (rest.at(-1))
      const m = /** @type {RegExpMatchArray} */ (Object.assign([hit], { index, input: testo }))
      return DOC_IP_PREFIXES.some(d => hit.startsWith(d)) || versioneXml(p, testo, m) ? hit : `[oscurato: ${name}]`
    })
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

module.exports = { scanFile, scanRepo, oscura, DOC_IP_PREFIXES }

if (require.main === module) {
  const found = scanRepo(path.resolve(__dirname, '..', '..'))
  if (found.length) { console.error('SEGRETI O FILE VIETATI:\n  ' + found.join('\n  ')); process.exit(1) }
  console.log('nessun segreto trovato nei file tracciati')
}
