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
/** Un pezzo di file (ADR-0005): la stessa costante di `m4tr1x_protocol::PEZZO` nell'app. */
const PEZZO_BYTES = 4 * 1024 * 1024
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
 * @property {((bytes: number) => void)} [onBlobServitoForTests] SOLO TEST: chiamata quando il nodo ha servito un file.
 *   È una funzione apposta: un file di configurazione (JSON) non può contenerla, quindi in produzione non si accende mai
 *   (decisione del fondatore, 20/10/2026: registrare ogni download con l'orario è un metadato da non tenere)
 * @property {string[]} anchors             ancore della directory: URL http://<onion v3>
 * @property {Partial<typeof import('./directory').DEFAULT_LIMITS>} anchorLimits  solo ruolo anchor
 * @property {number} descriptorTtlSec       scadenza della scheda del nodo (≤ 7 giorni)
 * @property {number} descriptorRenewSec     ogni quanto si rinnova
 * @property {number} torBootstrapTimeoutSec se tor non arriva al bootstrap completo entro questi secondi, il nodo non parte (errore: lo riavvia il sistema)
 * @property {((e: Error) => void)} [onFatal] chiamata se il nodo si ferma da solo per un guasto (es. tor terminato); il programma esce con errore e lo riavvia il sistema
 * @property {number} announceFallbackSec    se la pubblicazione dell'onion non è confermata, la scheda si annuncia comunque dopo questi secondi (un nodo non resta invisibile per sempre)
 * @property {number} descriptorPowBits      prova di lavoro della scheda (chiave nuova)
 * @property {string} [descriptorOnionForTests]
 * @property {string} [keyPassphrase]       passphrase del vault delle chiavi del nodo (K5, K13)
 * @property {string} [keyPassphraseFile]   oppure file che la contiene (fuori dalla cartella dati)
 * @property {{ maxEventBytes: number, perAuthorMaxEvents: number, perAuthorMaxBytes: number, maxTotalEvents: number, maxTotalBytes: number }} relayLimits
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
    // un blob = al massimo un PEZZO (ADR-0005, 4 MiB): i file più grandi arrivano a pezzi, ognuno un blob; così un
    // nodo piccolo non riceve mai un corpo più grande di questo e un caricamento interrotto riprende dai pezzi mancanti
    maxBlobBytes: c.maxBlobBytes ?? PEZZO_BYTES,
    quotaBytes: c.quotaBytes ?? 2 * 1024 * 1024 * 1024,
    peers: list(c.peers),
    allowLoopbackPeersForTests: !!c.allowLoopbackPeersForTests,
    // solo una funzione vera: `true` o una stringa (da un file di configurazione) si scartano
    onBlobServitoForTests: typeof c.onBlobServitoForTests === 'function' ? c.onBlobServitoForTests : undefined,
    anchors: list(c.anchors),
    anchorLimits: c.anchorLimits || {},
    descriptorTtlSec: c.descriptorTtlSec ?? 3 * 86400,
    descriptorRenewSec: c.descriptorRenewSec ?? 86400,
    announceFallbackSec: c.announceFallbackSec ?? 300,
    torBootstrapTimeoutSec: c.torBootstrapTimeoutSec ?? 180,
    onFatal: typeof c.onFatal === 'function' ? c.onFatal : undefined,
    descriptorPowBits: c.descriptorPowBits ?? 18,
    descriptorOnionForTests: c.descriptorOnionForTests,
    keyPassphrase: c.keyPassphrase,
    keyPassphraseFile: c.keyPassphraseFile,
    relayLimits: {
      maxEventBytes: 64 * 1024,
      perAuthorMaxEvents: 10000,
      perAuthorMaxBytes: 64 * 1024 * 1024,
      maxTotalEvents: 1000000,
      maxTotalBytes: 2 * 1024 * 1024 * 1024,
      ...(c.relayLimits || {}),
    },
    tor: c.tor !== false,
    torBin: c.torBin,
    log: c.log,
    adminKey: c.adminKey,
    allowNodeVersionMismatch: !!c.allowNodeVersionMismatch,
  }
  if (!(cfg.descriptorTtlSec >= 1 && cfg.descriptorTtlSec <= 7 * 86400)) throw new Error('descriptorTtlSec: da 1 secondo a 7 giorni')
  if (!(cfg.descriptorRenewSec >= 1 && cfg.descriptorRenewSec < cfg.descriptorTtlSec)) throw new Error('descriptorRenewSec deve essere minore della scadenza')
  if (!(cfg.announceFallbackSec >= 1 && cfg.announceFallbackSec <= 3600)) throw new Error('announceFallbackSec tra 1 e 3600')
  if (!(cfg.torBootstrapTimeoutSec >= 1 && cfg.torBootstrapTimeoutSec <= 3600)) throw new Error('torBootstrapTimeoutSec tra 1 e 3600')
  for (const o of cfg.allowedOrigins) {
    const u = new URL(o)
    if (u.origin !== o) throw new Error(`origine CORS non canonica: ${o}`)
  }
  return cfg
}

module.exports = { ROLES, PEZZO_BYTES, normalize }
