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
const APPROVED = new Set([])

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
