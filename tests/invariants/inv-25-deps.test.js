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

// Rust (app/), da F4: cargo deny e cargo audit in CI, con le stesse eccezioni e ognuna col suo motivo.
const APP = path.join(ROOT, 'app')
const idsTra = (/** @type {string} */ testo) => [...testo.matchAll(/RUSTSEC-\d{4}-\d{4}/g)].map(m => m[0]).sort()

test('cargo deny e cargo audit: stesse eccezioni, ognuna con il motivo', () => {
  const deny = fs.readFileSync(path.join(APP, 'deny.toml'), 'utf8')
  const audit = fs.readFileSync(path.join(APP, '.cargo', 'audit.toml'), 'utf8')
  const ignoraDeny = /\[advisories\][\s\S]*?\nignore\s*=\s*\[([\s\S]*?)\n\]/.exec(deny)
  const ignoraAudit = /\nignore\s*=\s*\[([\s\S]*?)\n\]/.exec(audit)
  assert.ok(ignoraDeny && ignoraAudit, 'elenco ignore mancante in deny.toml o audit.toml')
  assert.deepEqual(idsTra(ignoraAudit[1]), idsTra(ignoraDeny[1]), 'le eccezioni di cargo audit e cargo deny devono coincidere')
  for (const riga of ignoraDeny[1].split('\n').filter(r => /RUSTSEC-/.test(r)))
    assert.match(riga, /reason\s*=\s*"[^"]{20,}"/, `eccezione senza motivo scritto: ${riga.trim()}`)
  // gravità: gli avvisi non mantenuti/unsound contano, i crate ritirati bloccano
  assert.match(deny, /^yanked\s*=\s*"deny"/m)
  assert.match(deny, /^unmaintained\s*=\s*"all"/m)
  assert.match(deny, /^unsound\s*=\s*"all"/m)
  // solo crates.io
  assert.match(deny, /^unknown-registry\s*=\s*"deny"/m)
  assert.match(deny, /^unknown-git\s*=\s*"deny"/m)
  // I-36 sull'albero intero: WebSocket solo dal trasporto
  assert.match(deny, /\{\s*crate\s*=\s*"tokio-tungstenite",\s*wrappers\s*=\s*\["m4tr1x-transport"\]\s*\}/)
})

test('CI: cargo deny e cargo audit con versione e SHA-256 fissati, audit senza tolleranza', () => {
  const ci = fs.readFileSync(path.join(ROOT, '.github', 'workflows', 'ci.yml'), 'utf8')
  for (const [file, nome] of [['deny.tgz', 'cargo-deny'], ['audit.tgz', 'cargo-audit']]) {
    assert.match(ci, new RegExp(`echo "[0-9a-f]{64}  ${file.replace('.', '\\.')}" \\| sha256sum -c -`), `${nome}: hash non verificato prima dell'uso`)
  }
  assert.match(ci, /cargo-deny --locked check/)
  assert.match(ci, /cargo-audit audit --deny warnings/)
  assert.ok(!/cargo install cargo-(deny|audit)/.test(ci), 'strumenti compilati in CI invece dei binari con hash')
})
