// @ts-check
'use strict'
/**
 * Configurazione del nodo. Sicura di default (principio 1):
 * - in ascolto solo su 127.0.0.1 (le interfacce esterne richiedono allowExternalBind)
 * - nessuna origine CORS ammessa
 * - nessuno può caricare file finché non lo si elenca in `uploaders`
 * - peer solo .onion (il loopback esiste solo per i test)
 */
const { NETWORKS } = require('../../packages/protocol')

const ROLES = Object.freeze(['user', 'edge', 'hub', 'anchor'])
const LOOPBACK = new Set(['127.0.0.1', '::1', 'localhost'])

/**
 * @typedef {object} NodeConfig
 * @property {string} dataDir
 * @property {string} network               'm4tr1x-main' | 'm4tr1x-test'
 * @property {'user'|'edge'|'hub'|'anchor'} role
 * @property {string} host
 * @property {number} port                  0 = porta libera
 * @property {boolean} allowExternalBind
 * @property {string[]} allowedOrigins      origini CORS esatte
 * @property {string[]} allowedHosts        nomi Host ammessi oltre a quelli locali
 * @property {string[]} uploaders           pubkey (hex) che possono caricare file
 * @property {number} maxBlobBytes
 * @property {number} quotaBytes            per pubkey
 * @property {string[]} peers               URL http://<onion v3>[:porta]
 * @property {boolean} allowLoopbackPeersForTests
 * @property {string[]} anchors             ancore della directory: URL http://<onion v3>
 * @property {Partial<typeof import('./directory').DEFAULT_LIMITS>} anchorLimits  solo ruolo anchor
 * @property {number} descriptorTtlSec       scadenza della scheda del nodo (≤ 7 giorni)
 * @property {number} descriptorRenewSec     ogni quanto si rinnova
 * @property {number} descriptorPowBits      prova di lavoro della scheda (chiave nuova)
 * @property {string} [descriptorOnionForTests]
 * @property {boolean} tor
 * @property {string} [torBin]
 * @property {(line: string) => void} [log]
 * @property {string} [adminKey]
 * @property {boolean} [allowNodeVersionMismatch]
 */

/** @param {Partial<NodeConfig>} c @returns {NodeConfig} */
function normalize(c) {
  if (!c || typeof c.dataDir !== 'string' || !c.dataDir) throw new Error('dataDir richiesto')
  if (!NETWORKS.includes(c.network || '')) throw new Error(`rete sconosciuta: ${c.network}`)
  const role = c.role || 'user'
  if (!ROLES.includes(role)) throw new Error(`ruolo sconosciuto: ${role}`)
  const host = c.host || '127.0.0.1'
  if (!LOOPBACK.has(host) && !c.allowExternalBind)
    throw new Error(`ascolto su interfacce esterne (${host}) solo con allowExternalBind: true`)
  const list = (/** @type {any} */ v) => Array.isArray(v) ? v.map(String) : []
  const cfg = {
    dataDir: c.dataDir, network: /** @type {string} */ (c.network), role: /** @type {any} */ (role), host,
    port: Number.isInteger(c.port) ? /** @type {number} */ (c.port) : 8480,
    allowExternalBind: !!c.allowExternalBind,
    allowedOrigins: list(c.allowedOrigins),
    allowedHosts: list(c.allowedHosts).map(h => h.toLowerCase()),
    uploaders: list(c.uploaders).filter(p => /^[0-9a-f]{64}$/.test(p)),
    maxBlobBytes: c.maxBlobBytes ?? 50 * 1024 * 1024,
    quotaBytes: c.quotaBytes ?? 2 * 1024 * 1024 * 1024,
    peers: list(c.peers),
    allowLoopbackPeersForTests: !!c.allowLoopbackPeersForTests,
    anchors: list(c.anchors),
    anchorLimits: c.anchorLimits || {},
    descriptorTtlSec: c.descriptorTtlSec ?? 3 * 86400,
    descriptorRenewSec: c.descriptorRenewSec ?? 86400,
    descriptorPowBits: c.descriptorPowBits ?? 18,
    descriptorOnionForTests: c.descriptorOnionForTests,
    tor: c.tor !== false,
    torBin: c.torBin,
    log: c.log,
    adminKey: c.adminKey,
    allowNodeVersionMismatch: !!c.allowNodeVersionMismatch,
  }
  if (!(cfg.descriptorTtlSec >= 1 && cfg.descriptorTtlSec <= 7 * 86400)) throw new Error('descriptorTtlSec: da 1 secondo a 7 giorni')
  if (!(cfg.descriptorRenewSec >= 1 && cfg.descriptorRenewSec < cfg.descriptorTtlSec)) throw new Error('descriptorRenewSec deve essere minore della scadenza')
  for (const o of cfg.allowedOrigins) {
    const u = new URL(o)
    if (u.origin !== o) throw new Error(`origine CORS non canonica: ${o}`)
  }
  return cfg
}

module.exports = { ROLES, normalize }
