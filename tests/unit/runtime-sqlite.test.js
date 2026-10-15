// @ts-check
'use strict'
/**
 * Versione di Node fissata e funzioni di `node:sqlite` usate dal nodo.
 * node:sqlite è sperimentale in Node 22: se un aggiornamento ne cambia il
 * comportamento, questi test devono accorgersene prima dei nodi in produzione.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const os = require('os')
const fs = require('fs')
const path = require('path')
const { PINNED_NODE, assertPinnedNode } = require('../../node/src/runtime')

test('Node in uso = versione fissata in .node-version', () => {
  assert.equal(process.versions.node, PINNED_NODE, `usa Node ${PINNED_NODE} (es. npm run test:pinned)`)
  assert.doesNotThrow(() => assertPinnedNode())
})

test('il nodo rifiuta una versione di Node diversa', () => {
  const real = Object.getOwnPropertyDescriptor(process.versions, 'node')
  Object.defineProperty(process.versions, 'node', { value: '99.0.0', configurable: true })
  try { assert.throws(() => assertPinnedNode(), /fissata/) }
  finally { Object.defineProperty(process.versions, 'node', /** @type {PropertyDescriptor} */ (real)) }
})

test('node:sqlite: le funzioni usate dal nodo si comportano come atteso', () => {
  const { openDb } = require('../../node/src/db')
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'm4-sqlite-'))
  const db = openDb(dir)
  // WAL e chiavi esterne attive
  assert.equal(/** @type {any} */ (db.prepare('PRAGMA journal_mode').get()).journal_mode, 'wal')
  assert.equal(/** @type {any} */ (db.prepare('PRAGMA foreign_keys').get()).foreign_keys, 1)
  // INSERT OR IGNORE + changes
  const ins = db.prepare('INSERT OR IGNORE INTO events (id,pubkey,created_at,kind,tags,content,sig,received_at) VALUES (?,?,?,?,?,?,?,?)')
  assert.equal(ins.run('e1', 'p', 1, 1, '[]', 'c', 's', 1).changes, 1)
  assert.equal(ins.run('e1', 'p', 1, 1, '[]', 'c', 's', 1).changes, 0)
  // cascata delle chiavi esterne
  db.prepare('INSERT INTO event_tags (event_id,name,value) VALUES (?,?,?)').run('e1', 't', 'x')
  db.prepare('DELETE FROM events WHERE id = ?').run('e1')
  assert.equal(/** @type {any} */ (db.prepare('SELECT COUNT(*) AS n FROM event_tags').get()).n, 0)
  // upsert con MAX
  const up = db.prepare('INSERT INTO peer_state (peer, since) VALUES (?, ?) ON CONFLICT(peer) DO UPDATE SET since = MAX(since, excluded.since)')
  up.run('a', 10); up.run('a', 5); up.run('a', 20)
  assert.equal(/** @type {any} */ (db.prepare('SELECT since FROM peer_state WHERE peer = ?').get('a')).since, 20)
  // parametri variabili e all()
  assert.equal(db.prepare('SELECT ? AS a, ? AS b').all(...[1, 2]).length, 1)
  db.close()
})
