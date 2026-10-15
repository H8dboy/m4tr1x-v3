#!/usr/bin/env node
// @ts-check
'use strict'
/**
 * Cerimonia delle chiavi (KEYS.md): genera la chiave di release K11 (ML-DSA-65).
 * Da eseguire SOLO su un computer offline.
 *
 * Uso: M4TR1X_PASSPHRASE='...' node tools/release/keygen.js <cartella-di-uscita>
 * Scrive:
 *   release-key.sealed.json  segreto sigillato (scrypt N=2^17 + AES-GCM): resta offline
 *   release-key.pub          chiave pubblica: va copiata in tools/release/ nel repo
 */
const fs = require('fs')
const path = require('path')
const keys = require('../../packages/keys')

const out = process.argv[2]
const pass = process.env.M4TR1X_PASSPHRASE || ''
if (!out) { console.error('uso: keygen.js <cartella-di-uscita>'); process.exit(2) }
if (pass.length < 16) { console.error('passphrase di almeno 16 caratteri in M4TR1X_PASSPHRASE'); process.exit(2) }
fs.mkdirSync(out, { recursive: true })
const sealedPath = path.join(out, 'release-key.sealed.json')
if (fs.existsSync(sealedPath)) { console.error('esiste già una chiave in questa cartella: non la sovrascrivo'); process.exit(1) }
const k = keys.generateKey('release')
fs.writeFileSync(sealedPath, keys.sealSecret(keys.exportSecret(k), pass), { mode: 0o600 })
fs.writeFileSync(path.join(out, 'release-key.pub'), k.publicKey + '\n')
console.log('chiave di release generata. Impronta (sha256 della chiave pubblica):')
console.log(require('crypto').createHash('sha256').update(k.publicKey).digest('hex'))
