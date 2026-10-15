// @ts-check
'use strict'
/**
 * Database del nodo: SQLite incluso in Node (`node:sqlite`), nessuna dipendenza
 * nativa da compilare. È sperimentale in Node 22: per questo la versione di Node
 * è fissata (runtime.js) e un test controlla le funzioni che usiamo.
 */
const fs = require('fs')
const path = require('path')
const { DatabaseSync } = require('node:sqlite')

/** @param {string} dataDir */
function openDb(dataDir) {
  fs.mkdirSync(dataDir, { recursive: true })
  const db = new DatabaseSync(path.join(dataDir, 'node.db'))
  db.exec(`
    PRAGMA journal_mode = WAL;
    PRAGMA synchronous = NORMAL;
    PRAGMA foreign_keys = ON;
    CREATE TABLE IF NOT EXISTS events (
      id TEXT PRIMARY KEY, pubkey TEXT NOT NULL, created_at INTEGER NOT NULL,
      kind INTEGER NOT NULL, tags TEXT NOT NULL, content TEXT NOT NULL, sig TEXT NOT NULL,
      received_at INTEGER NOT NULL,
      expires_at INTEGER,          -- NIP-40: dopo questa data l'evento non esiste più
      dtag TEXT                    -- NIP-33: chiave degli eventi sostituibili parametrizzati
    );
    CREATE INDEX IF NOT EXISTS ev_expires ON events(expires_at) WHERE expires_at IS NOT NULL;
    CREATE INDEX IF NOT EXISTS ev_replace ON events(pubkey, kind, dtag);
    CREATE INDEX IF NOT EXISTS ev_kind ON events(kind, created_at);
    CREATE INDEX IF NOT EXISTS ev_pubkey ON events(pubkey, created_at);
    CREATE INDEX IF NOT EXISTS ev_created ON events(created_at);
    CREATE TABLE IF NOT EXISTS event_tags (
      event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
      name TEXT NOT NULL, value TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS tag_lookup ON event_tags(name, value);
    CREATE TABLE IF NOT EXISTS blobs (
      sha256 TEXT PRIMARY KEY, size INTEGER NOT NULL, pubkey TEXT NOT NULL, created_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS blob_owner ON blobs(pubkey);
    CREATE TABLE IF NOT EXISTS peer_state (peer TEXT PRIMARY KEY, since INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS directory_keys (
      pubkey TEXT PRIMARY KEY, first_seen INTEGER NOT NULL, last_update INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS dk_first ON directory_keys(first_seen);
  `)
  return db
}

module.exports = { openDb }
