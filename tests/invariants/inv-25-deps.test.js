// @ts-check
'use strict'
/**
 * I-25 — Dipendenze minime e controllate.
 * (1) nessun package.json del repo dichiara pacchetti vietati;
 * (2) ogni dipendenza dichiarata è nell'elenco approvato (aggiungerne una
 *     richiede di modificare questo elenco, cioè una scelta esplicita in review).
 * In CI gira anche `npm audit --audit-level=high` (e `cargo deny` dalla F4).
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const { execFileSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..', '..')

/** Vietati: P2P che espone IP, client HTTP che scavalcano Tor nelle app, federazioni tolte. */
const BANNED = ['webtorrent', 'bittorrent-tracker', 'simple-peer', 'request', 'node-fetch', 'axios', 'got',
  'electron', 'electron-builder', '@capacitor/core', 'pg', 'activitypub-express', 'masto', 'megalodon']

/** Approvate (si allunga solo con un motivo scritto nel commit). */
const APPROVED = new Set([
  // F1: firme Schnorr secp256k1 (Nostr), hash, HKDF, BIP-39. Librerie con audit
  // indipendenti, zero dipendenze esterne oltre @noble/hashes.
  '@noble/curves', '@noble/hashes', '@scure/bip39',
  // ADR-0017: ML-DSA-65 per wallet, emittente e release (stesso autore, stesse @noble/*).
  '@noble/post-quantum',
  // F2: WebSocket del relay Nostr (zero dipendenze).
  'ws',
  // F2, solo sviluppo: controllo dei tipi in CI (ADR-0012), non finiscono nei pacchetti.
  'typescript', '@types/node', '@types/ws',
])

function packageJsons() {
  return execFileSync('git', ['ls-files', '-z', '--', '*package.json'], { cwd: ROOT }).toString('utf8')
    .split('\0').filter(Boolean).map(f => path.join(ROOT, f))
}

test('nessun pacchetto vietato', () => {
  for (const p of packageJsons()) {
    const j = JSON.parse(fs.readFileSync(p, 'utf8'))
    const all = { ...j.dependencies, ...j.devDependencies, ...j.optionalDependencies }
    for (const name of Object.keys(all)) assert.ok(!BANNED.includes(name), `${path.relative(ROOT, p)}: ${name} è vietato`)
  }
})

test('ogni dipendenza è approvata', () => {
  for (const p of packageJsons()) {
    const j = JSON.parse(fs.readFileSync(p, 'utf8'))
    const all = { ...j.dependencies, ...j.devDependencies, ...j.optionalDependencies }
    for (const name of Object.keys(all)) assert.ok(APPROVED.has(name), `${path.relative(ROOT, p)}: ${name} non è nell'elenco approvato`)
  }
})

/** Ogni GitHub Action è fissata per hash del commit (40 hex), mai per tag o ramo:
 *  un tag si può spostare su codice nuovo, un hash no. Il commento "# vX.Y.Z" serve solo a chi legge. */
test('GitHub Actions fissate per hash del commit', () => {
  const files = execFileSync('git', ['ls-files', '-z', '--', '.github/workflows/*.yml', '.github/workflows/*.yaml', '.github/actions/**'], { cwd: ROOT })
    .toString('utf8').split('\0').filter(Boolean)
  assert.ok(files.length > 0, 'nessun workflow trovato')
  let seen = 0
  for (const f of files) {
    const lines = fs.readFileSync(path.join(ROOT, f), 'utf8').split(/\r?\n/)
    lines.forEach((line, i) => {
      const m = line.match(/^\s*(?:-\s*)?uses:\s*['"]?([^'"\s#]+)/)
      if (!m) return
      seen++
      const ref = m[1]
      if (ref.startsWith('./')) return // action locale del repo: segue il commit stesso
      assert.match(ref, /^[\w.-]+\/[\w./-]+@[0-9a-f]{40}$/, `${f}:${i + 1}: "${ref}" non è fissata per hash del commit`)
    })
  }
  assert.ok(seen > 0, 'nessun "uses:" trovato: il controllo non sta guardando niente')
})
