// @ts-check
'use strict'
/**
 * Versione di Node fissata (file .node-version alla radice del repo).
 * Il nodo usa `node:sqlite`, che in Node 22 è ancora sperimentale: un
 * aggiornamento di Node potrebbe cambiarne l'API. Il nodo quindi parte solo
 * con la versione esatta su cui è stato testato.
 */
const fs = require('fs')
const path = require('path')

const PINNED_NODE = fs.readFileSync(path.join(__dirname, '..', '..', '.node-version'), 'utf8').trim()

/** @param {{ allowNodeVersionMismatch?: boolean }} [opts] */
function assertPinnedNode(opts = {}) {
  if (process.versions.node === PINNED_NODE || opts.allowNodeVersionMismatch) return
  throw new Error(`versione di Node ${process.versions.node} diversa da quella fissata ${PINNED_NODE} (.node-version): ` +
    'il nodo usa node:sqlite, sperimentale in Node 22. Usa la versione fissata.')
}

module.exports = { PINNED_NODE, assertPinnedNode }
