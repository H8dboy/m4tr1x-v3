// @ts-check
'use strict'
/**
 * Avvio del nodo M4TR1X v3 (ruoli user e anchor; edge e hub dalle fasi successive).
 */
const fs = require('fs')
const path = require('path')
const http = require('http')
const crypto = require('crypto')
const { normalize } = require('./config')
const { makeLogger } = require('./log')
const { openDb } = require('./db')
const { createRelay } = require('./relay')
const { createHttp } = require('./http')
const { creaMetriche } = require('./metriche')
const { startPeers } = require('./peers')
const { startTor, attendiAnnuncio } = require('./tor')
const { assertPinnedNode } = require('./runtime')
const keys = require('../../packages/keys')
const directory = require('./directory')

const VERSION = '3.0.0-dev'

/**
 * Passphrase del vault: dalla configurazione, da un file (consigliato: fuori dalla
 * cartella dati, es. credenziale di systemd) o dalla variabile M4TR1X_KEY_PASSPHRASE.
 * Nessun default: senza passphrase il nodo non parte (principio 1).
 * @param {import('./config').NodeConfig} cfg
 */
function keyPassphrase(cfg) {
  if (cfg.keyPassphrase) return cfg.keyPassphrase
  if (cfg.keyPassphraseFile) return fs.readFileSync(cfg.keyPassphraseFile, 'utf8').trim()
  if (process.env.M4TR1X_KEY_PASSPHRASE) return process.env.M4TR1X_KEY_PASSPHRASE
  throw new Error('passphrase delle chiavi del nodo mancante: keyPassphrase, keyPassphraseFile o M4TR1X_KEY_PASSPHRASE')
}

/**
 * Chiavi del nodo K5 (scheda della directory), K13 (admin) e K6 (chiave dell'onion, dal
 * 16/10/2026: prima era un file in chiaro nella cartella di tor), in un vault cifrato
 * (scrypt N=2^17 + AES-GCM). Mai in chiaro nella cartella dati (I-06). I file in
 * chiaro di versioni precedenti vengono sigillati e cancellati.
 * @param {string} dataDir @param {string} pass
 */
function loadSecrets(dataDir, pass) {
  const vault = path.join(dataDir, 'secrets.vault.json')
  const oldNode = path.join(dataDir, 'node.key'), oldAdmin = path.join(dataDir, 'admin.key')
  /** @type {{ node: string, admin: string, onion?: string }} */ let s
  if (fs.existsSync(vault)) {
    try { s = JSON.parse(Buffer.from(keys.openSecret(fs.readFileSync(vault, 'utf8'), pass)).toString('utf8')) }
    catch { throw new Error('vault delle chiavi: passphrase sbagliata o file alterato') }
  } else {
    s = {
      node: fs.existsSync(oldNode) ? fs.readFileSync(oldNode, 'utf8').trim() : Buffer.from(keys.exportSecret(keys.generateNodeKey())).toString('hex'),
      admin: fs.existsSync(oldAdmin) ? fs.readFileSync(oldAdmin, 'utf8').trim() : crypto.randomBytes(32).toString('hex'),
    }
    sigilla(vault, s, pass)
  }
  for (const f of [oldNode, oldAdmin]) {
    if (!fs.existsSync(f)) continue
    try { fs.writeFileSync(f, crypto.randomBytes(fs.statSync(f).size)) } catch {}   // sovrascrittura prima di cancellare
    fs.rmSync(f, { force: true })
  }
  return {
    nodeKey: keys.roleKeyFromSecret(keys.NODE_ROLE, 1, Buffer.from(s.node, 'hex')), adminKey: s.admin,
    /** K6: chiave dell'onion per ADD_ONION (blob di tor), o null al primo avvio */
    onionKey: s.onion || null,
    /** Al primo avvio tor genera K6: la si sigilla subito nel vault. @param {string} k */
    saveOnionKey: k => { s.onion = k; sigilla(vault, s, pass) },
  }
}

/** Scrive il vault in modo atomico (file temporaneo + rinomina), permessi 600. */
function sigilla(/** @type {string} */ vault, /** @type {object} */ s, /** @type {string} */ pass) {
  const tmp = vault + '.tmp'
  fs.writeFileSync(tmp, keys.sealSecret(Buffer.from(JSON.stringify(s)), pass), { mode: 0o600 })
  fs.renameSync(tmp, vault)
}

/** @param {http.Server} srv @param {string} host @param {number} port @returns {Promise<number>} */
function listen(srv, host, port) {
  return new Promise((resolve, reject) => {
    srv.once('error', reject)
    srv.listen(port, host, () => { srv.off('error', reject); resolve(/** @type {import('net').AddressInfo} */ (srv.address()).port) })
  })
}

/** @param {Partial<import('./config').NodeConfig>} input */
async function startNode(input) {
  const cfg = normalize(input)
  assertPinnedNode(cfg)
  const log = makeLogger(cfg.log)
  fs.mkdirSync(cfg.dataDir, { recursive: true, mode: 0o700 })
  const db = openDb(cfg.dataDir)
  const secrets = loadSecrets(cfg.dataDir, keyPassphrase(cfg))
  const adminKey = cfg.adminKey || secrets.adminKey
  let onion = /** @type {string|null} */ (null)
  let socksPort = /** @type {number|null} */ (null)

  const nodeKey = secrets.nodeKey
  const policy = cfg.role === 'anchor' ? directory.anchorPolicy({ db, network: cfg.network, limits: cfg.anchorLimits }) : undefined
  const metriche = creaMetriche()
  const relay = createRelay({ db, network: cfg.network, log, policy, limits: cfg.relayLimits, trusted: cfg.uploaders, metriche })
  const web = createHttp({ cfg, db, log, adminKey, onion: () => onion, version: VERSION, metriche, connessioni: relay.connessioni })

  /** @param {boolean} isPublic */
  const makeServer = isPublic => {
    const srv = http.createServer({ requestTimeout: 120000, headersTimeout: 15000, maxHeaderSize: 16384 }, (req, res) => {
      web.handle(req, res, isPublic).catch(() => { try { res.writeHead(500); res.end() } catch {} })
    })
    srv.on('upgrade', (req, socket, head) => {
      socket.on('error', () => {})
      const p = (req.url || '').split('?')[0]
      if (!web.hostAllowed(req, isPublic)) { socket.end('HTTP/1.1 421 Misdirected Request\r\nConnection: close\r\n\r\n'); return }
      if (p !== '/relay') { socket.end('HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n'); return }
      relay.wss.handleUpgrade(req, socket, head, ws => relay.wss.emit('connection', ws, req))
    })
    srv.on('clientError', (_e, socket) => { try { socket.end('HTTP/1.1 400 Bad Request\r\n\r\n') } catch {} })
    return srv
  }

  const owner = makeServer(false)
  const pub = makeServer(true)
  const port = await listen(owner, cfg.host, cfg.port)
  const ingressPort = await listen(pub, '127.0.0.1', 0)
  log(`nodo avviato: ruolo ${cfg.role}, rete ${cfg.network}, porta ${port}${cfg.host === '127.0.0.1' || cfg.host === 'localhost' || cfg.host === '::1' ? ' (solo locale)' : ' (ESPOSTO)'}`)

  let tor = /** @type {Awaited<ReturnType<typeof startTor>> | null} */ (null)
  /** @type {any} */ let api = null
  if (cfg.tor) {
    if (!cfg.torBin) throw new Error('tor: indicare torBin (binario tor verificato)')
    try {
      tor = await startTor({
        bin: cfg.torBin, dataDir: cfg.dataDir, ingressPort, log, timeoutMs: cfg.torBootstrapTimeoutSec * 1000,
        chiaveOnion: secrets.onionKey, salvaChiaveOnion: secrets.saveOnionKey,
        // tor morto con il nodo acceso: niente riavvio qui dentro, il nodo si chiude con errore
        // e lo riavvia il sistema (systemd Restart=on-failure, servizio Windows)
        onMorto: codice => {
          log('nodo: tor terminato, il nodo si chiude con errore (lo riavvia il sistema)')
          const err = new Error(`tor terminato con il nodo acceso (codice ${codice})`)
          Promise.resolve(api?.close()).catch(() => {}).finally(() => cfg.onFatal?.(err))
        },
      })
    } catch (e) {
      // bootstrap non completato o onion rifiutato: si chiude ciò che era già aperto, poi errore
      log(`nodo: avvio di tor fallito (${/** @type {Error} */ (e).message}), il nodo non parte (lo riavvia il sistema)`)
      relay.close()
      await Promise.all([owner, pub].map(s => new Promise(r => { s.closeAllConnections?.(); s.close(() => r(undefined)) })))
      try { db.close() } catch {}
      throw e
    }
    onion = tor.onion
    socksPort = tor.socksPort
  }

  const peers = startPeers({ cfg, db, relay, log, socksPort: () => socksPort })
  // La scheda nella directory parte solo quando l'onion è davvero trovabile (tor.js:
  // ogni UPLOAD con risposta e almeno 6 HSDir con UPLOADED). Senza tor, subito.
  /** @type {ReturnType<typeof directory.startPublisher> | null} */ let publisher = null
  let closed = false
  /** @type {ReturnType<typeof attendiAnnuncio> | null} */ let attesaAnnuncio = null
  const avviaPublisher = () => {
    if (!closed && !publisher) publisher = directory.startPublisher({ cfg, nodeKey, onion: () => onion, socksPort: () => socksPort, log, version: VERSION })
  }
  if (tor) {
    log(`directory: scheda in attesa della pubblicazione dell'onion (al massimo ${cfg.announceFallbackSec} s)`)
    attesaAnnuncio = attendiAnnuncio(tor.published, cfg.announceFallbackSec * 1000)
    attesaAnnuncio.esito.then(esito => {
      if (esito === 'scadenza') log('directory: pubblicazione dell\'onion non confermata, annuncio comunque la scheda')
      avviaPublisher()
    })
  } else avviaPublisher()

  api = {
    port, ingressPort, adminKey,
    get onion() { return onion },
    /** Si risolve quando il descrittore del nostro onion è pubblicato (tor.js: almeno 6 HSDir del periodo attuale). */
    onionPublished: tor ? tor.published : Promise.resolve(),
    /** Solo per i test (I-59): stato della pubblicazione e porta di controllo del tor del nodo. */
    _torForTests: tor ? { stato: tor.statoPubblicazione, attualeNoto: tor.attualeNoto, controlPort: tor.controlPort, cookieFile: tor.cookieFile, chiudiControllo: tor.chiudiControlloPerProva, pid: tor.pid } : null,
    /** Quando e perché la scheda viene annunciata: 'confermata' (pubblicazione) o 'scadenza' (riserva). Senza tor: subito. */
    announceDecision: attesaAnnuncio ? attesaAnnuncio.esito : Promise.resolve(/** @type {'confermata'|'scadenza'} */ ('confermata')),
    relay, db,
    nodePubkey: nodeKey.publicKey,
    /** Solo per i test (I-06): il segreto della chiave del nodo, per cercarlo su disco. */
    _nodeSecretHexForTests: () => Buffer.from(keys.exportSecret(nodeKey)).toString('hex'),
    directory: { purgeExpired: () => relay.purgeExpired(), publishNow: () => publisher ? publisher.publishOnce() : Promise.resolve(false) },
    /** @param {string[]} list */ addPeers: list => peers.add(list),
    async close() {
      if (closed) return   // può chiamarla sia chi ha avviato il nodo sia la morte di tor
      closed = true
      attesaAnnuncio?.annulla()
      publisher?.stop()
      peers.stop()
      relay.close()
      await Promise.all([owner, pub].map(s => new Promise(r => { s.closeAllConnections?.(); s.close(() => r(undefined)) })))
      if (tor) tor.stop()
      try { db.close() } catch {}
      log('nodo fermato')
    },
  }
  return api
}

module.exports = { startNode, VERSION }
