// @ts-check
'use strict'
/**
 * I-06 (lato nodo) — Le chiavi del nodo (K5) e admin (K13) non stanno mai in
 * chiaro nella cartella dati: sono in un vault cifrato (scrypt N=2^17 + AES-GCM).
 * - il nodo non parte senza passphrase (nessun default insicuro)
 * - con la passphrase sbagliata non parte
 * - nessun file della cartella dati contiene le chiavi
 * - eventuali file in chiaro di versioni precedenti vengono sigillati e rimossi
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const os = require('os')
const path = require('path')
const crypto = require('crypto')
const { NET } = require('../helpers/node')

const { startNode } = require('../../node')
const PASS = 'passphrase del nodo di prova, lunga'

function walk(/** @type {string} */ d) {
  return fs.readdirSync(d, { withFileTypes: true }).flatMap(e => e.isDirectory() ? walk(path.join(d, e.name)) : [path.join(d, e.name)])
}

test('senza passphrase il nodo non parte', async () => {
  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'm4-sec-'))
  await assert.rejects(() => startNode({ dataDir, network: NET, port: 0, tor: false, log: () => {} }), /passphrase/)
})

test('nella cartella dati nessuna chiave in chiaro; con passphrase sbagliata non riparte', async () => {
  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'm4-sec-'))
  const n = await startNode({ dataDir, network: NET, port: 0, tor: false, log: () => {}, keyPassphrase: PASS })
  const admin = n.adminKey
  const nodeSecretHex = n._nodeSecretHexForTests()
  await n.close()
  for (const f of walk(dataDir)) {
    const raw = fs.readFileSync(f)
    for (const needle of [admin, nodeSecretHex, Buffer.from(nodeSecretHex, 'hex').toString('base64')]) {
      assert.ok(!raw.toString('latin1').includes(needle), `chiave in chiaro in ${path.relative(dataDir, f)}`)
    }
    assert.ok(!raw.includes(Buffer.from(nodeSecretHex, 'hex')), `byte della chiave del nodo in ${path.relative(dataDir, f)}`)
  }
  assert.ok(!fs.existsSync(path.join(dataDir, 'node.key')) && !fs.existsSync(path.join(dataDir, 'admin.key')))
  await assert.rejects(() => startNode({ dataDir, network: NET, port: 0, tor: false, log: () => {}, keyPassphrase: 'sbagliata' }), /passphrase|vault/)
  const again = await startNode({ dataDir, network: NET, port: 0, tor: false, log: () => {}, keyPassphrase: PASS })
  assert.equal(again.adminKey, admin, 'con la passphrase giusta le chiavi sono le stesse')
  await again.close()
})

test('file in chiaro di una versione precedente: sigillati e rimossi', async () => {
  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'm4-sec-'))
  const oldAdmin = crypto.randomBytes(32).toString('hex')
  fs.writeFileSync(path.join(dataDir, 'admin.key'), oldAdmin)
  const n = await startNode({ dataDir, network: NET, port: 0, tor: false, log: () => {}, keyPassphrase: PASS })
  assert.equal(n.adminKey, oldAdmin, 'la chiave admin esistente viene conservata')
  await n.close()
  assert.ok(!fs.existsSync(path.join(dataDir, 'admin.key')), 'il file in chiaro deve sparire')
})
