// @ts-check
'use strict'
/**
 * I-28 — Nessun segreto nel repo.
 * (1) lo scanner riconosce ogni tipo di segreto vietato (esempi finti qui sotto);
 * (2) i file tracciati oggi non ne contengono.
 * In CI gira anche gitleaks sulla storia intera.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const path = require('path')
const { scanFile, scanRepo } = require('../../tools/security/scan-secrets')

const FAKE = {
  nsec: 'nsec1' + 'q'.repeat(58),
  pem: '-----BEGIN PRIVATE KEY-----',
  onion: 'a'.repeat(56) + '.onion',
  lan: 'http://192.168.1.7:8080',
  pub: 'ssh utente@8.8.4.4',
  mail: 'qualcuno@protonmail.com',
  hex64: 'ab'.repeat(64),
}

for (const [name, sample] of Object.entries(FAKE)) {
  test(`riconosce: ${name}`, () => {
    assert.ok(scanFile('src/x.js', `const v = "${sample}"`).length > 0)
  })
}

for (const f of ['.env', 'config/.env.local', 'data/relay.db', 'h8ledger.db-wal', 'tor/hs_ed25519_secret_key',
  'release.keystore', 'node_config.json', 'h8identity.enc', 'app-release.apk']) {
  test(`file vietato: ${f}`, () => assert.ok(scanFile(f, '').length > 0))
}

test('ammessi: email noreply, i tre blocchi di documentazione (RFC 5737), localhost', () => {
  const ok = 'Co-Authored-By: x <234557612+H8dboy@users.noreply.github.com> 127.0.0.1 192.0.2.10 198.51.100.4 203.0.113.5 version 3.0.0'
  assert.deepEqual(scanFile('docs/x.md', ok), [])
})

test('un IP vero accanto a uno di documentazione viene comunque segnalato', () => {
  assert.ok(scanFile('src/x.js', 'a = "203.0.113.5"; b = "' + FAKE.pub.split('@')[1] + '"').length > 0)
})

test('il repo non contiene segreti nei file tracciati', () => {
  const found = scanRepo(path.resolve(__dirname, '..', '..'))
  assert.deepEqual(found, [], found.join('\n'))
})
