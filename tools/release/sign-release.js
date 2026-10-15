#!/usr/bin/env node
// @ts-check
'use strict'
/**
 * Firma offline di una release con la chiave K11 (ML-DSA-65, ADR-0017).
 * La chiave non entra MAI nella CI: si firma sul computer offline e si
 * pubblica solo release.json.sig.
 *
 * Uso: M4TR1X_PASSPHRASE='...' node tools/release/sign-release.js <release-key.sealed.json> <cartella-release> [rete]
 */
const fs = require('fs')
const path = require('path')
const keys = require('../../packages/keys')
const protocol = require('../../packages/protocol')
const { verifyReleaseDir, signatureFile, DOMAIN } = require('./verify-release')

const [sealedPath, dir, network = 'm4tr1x-main'] = process.argv.slice(2)
if (!sealedPath || !dir) { console.error('uso: sign-release.js <chiave-sigillata> <cartella-release> [rete]'); process.exit(2) }
const secret = keys.openSecret(fs.readFileSync(sealedPath, 'utf8'), process.env.M4TR1X_PASSPHRASE || '')
const k = keys.roleKeyFromSecret('release', 1, secret)
secret.fill(0)
const manifest = fs.readFileSync(path.join(dir, 'release.json'))
fs.writeFileSync(path.join(dir, 'release.json.sig'), signatureFile(protocol.signDomain(k, DOMAIN, network, manifest), network))
const m = verifyReleaseDir(dir, k.publicKey, network)
console.log(`release ${m.version} (seq ${m.seq}) firmata e verificata`)
