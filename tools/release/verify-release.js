#!/usr/bin/env node
// @ts-check
'use strict'
/**
 * Cancello delle release (invariante I-23). Firma: ML-DSA-65 con la chiave di
 * release K11, dominio "release/manifest" (ADR-0017).
 *
 * Uso: node tools/release/verify-release.js <cartella-release> [rete]
 * La cartella deve contenere release.json, release.json.sig e tutti i pacchetti
 * elencati. Esce con codice ≠ 0 (e la CI non pubblica) se:
 *   - manca la chiave pubblica di release nel repo (cerimonia non ancora fatta)
 *   - manca la firma, o non è valida, o è di un'altra rete
 *   - un pacchetto elencato manca o ha un hash diverso
 *   - c'è un file nella cartella che il manifest non elenca
 */
const fs = require('fs')
const path = require('path')
const crypto = require('crypto')
const protocol = require('../../packages/protocol')

const PUBKEY_PATH = path.join(__dirname, 'release-key.pub')
const DOMAIN = 'release/manifest'

/** Contenuto del file release.json.sig. @param {string} sigHex @param {string} network */
function signatureFile(sigHex, network) {
  return JSON.stringify({ alg: 'ML-DSA-65', domain: DOMAIN, network, sig: sigHex }) + '\n'
}

/** @param {string} dir @param {string} [pubHex] @param {string} [network] */
function verifyReleaseDir(dir, pubHex, network = 'm4tr1x-main') {
  if (pubHex === undefined) {
    if (!fs.existsSync(PUBKEY_PATH)) throw new Error('chiave pubblica di release assente (tools/release/release-key.pub): nessuna release pubblicabile prima della cerimonia delle chiavi')
    pubHex = fs.readFileSync(PUBKEY_PATH, 'utf8').trim()
  }
  if (pubHex.length !== 1952 * 2) throw new Error('la chiave di release deve essere ML-DSA-65')
  const manifestPath = path.join(dir, 'release.json')
  const sigPath = path.join(dir, 'release.json.sig')
  if (!fs.existsSync(manifestPath)) throw new Error('release.json assente')
  if (!fs.existsSync(sigPath)) throw new Error('release.json.sig assente: release non firmata')
  const manifestBytes = fs.readFileSync(manifestPath)
  let sig
  try { sig = JSON.parse(fs.readFileSync(sigPath, 'utf8')) } catch { throw new Error('release.json.sig illeggibile') }
  if (sig.alg !== 'ML-DSA-65' || sig.domain !== DOMAIN) throw new Error('firma di formato sconosciuto')
  if (sig.network !== network) throw new Error(`firma per la rete "${sig.network}", attesa "${network}"`)
  if (!protocol.verifyDomain(pubHex, String(sig.sig), DOMAIN, network, manifestBytes)) throw new Error('firma non valida')

  /** @type {{ version: string, seq: number, files: Record<string, string> }} */
  const manifest = JSON.parse(manifestBytes.toString('utf8'))
  if (typeof manifest.version !== 'string' || !Number.isInteger(manifest.seq) || !manifest.files || typeof manifest.files !== 'object')
    throw new Error('release.json: campi version/seq/files mancanti o errati')
  const listed = new Set(Object.keys(manifest.files))
  for (const [name, sha] of Object.entries(manifest.files)) {
    if (name.includes('/') || name.includes('\\') || name.startsWith('.')) throw new Error(`nome di file non ammesso: ${name}`)
    const p = path.join(dir, name)
    if (!fs.existsSync(p)) throw new Error(`pacchetto elencato ma assente: ${name}`)
    const actual = crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex')
    if (actual !== String(sha).toLowerCase()) throw new Error(`hash diverso per ${name}`)
  }
  for (const f of fs.readdirSync(dir)) {
    if (f === 'release.json' || f === 'release.json.sig') continue
    if (!listed.has(f)) throw new Error(`file non elencato nel manifest: ${f}`)
  }
  return manifest
}

module.exports = { verifyReleaseDir, signatureFile, DOMAIN }

if (require.main === module) {
  const dir = process.argv[2]
  if (!dir) { console.error('uso: verify-release.js <cartella-release> [rete]'); process.exit(2) }
  try {
    const m = verifyReleaseDir(dir, undefined, process.argv[3] || 'm4tr1x-main')
    console.log(`release ${m.version} (seq ${m.seq}) verificata: ${Object.keys(m.files).length} pacchetti`)
  } catch (e) {
    console.error('RELEASE RIFIUTATA:', /** @type {Error} */ (e).message)
    process.exit(1)
  }
}
