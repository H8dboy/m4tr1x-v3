// @ts-check
'use strict'
/**
 * Avvio del nodo da un file di configurazione (per il servizio di sistema).
 *
 * In produzione la configurazione si legge SOLO da un file JSON (RIPRESA, punto 25):
 * - un JSON non può contenere funzioni, quindi nessuna opzione `…ForTests` che sia una funzione può accendersi;
 * - qui in più si rifiuta ogni chiave che non sia nell'elenco chiuso qui sotto: le opzioni di prova, la
 *   passphrase in chiaro, la chiave d'amministrazione scelta a mano e il permesso di cambiare versione di
 *   Node NON si possono scrivere nel file (un errore di battitura non diventa un default silenzioso);
 * - la passphrase del vault sta in un file a parte, fuori dalla cartella dati e leggibile solo dal
 *   proprietario (su Linux/macOS si controllano i permessi).
 */
const fs = require('fs')
const path = require('path')

/** Le sole chiavi ammesse nel file. Tutto il resto è un errore. */
const CHIAVI_AMMESSE = Object.freeze([
  'dataDir', 'network', 'role', 'host', 'port', 'allowExternalBind', 'allowedOrigins', 'allowedHosts',
  'uploaders', 'maxBlobBytes', 'quotaBytes', 'peers', 'anchors', 'anchorLimits',
  'descriptorTtlSec', 'descriptorRenewSec', 'torBootstrapTimeoutSec', 'announceFallbackSec', 'descriptorPowBits',
  'keyPassphraseFile', 'relayLimits', 'tor', 'torBin',
])

/**
 * Legge e controlla il file di configurazione. Lancia un errore che dice cosa non va.
 * @param {string} file
 * @returns {Partial<import('./config').NodeConfig>}
 */
function leggiConfig(file) {
  let grezzo
  try { grezzo = fs.readFileSync(file, 'utf8') } catch { throw new Error(`configurazione: file non leggibile (${file})`) }
  /** @type {any} */ let c
  try { c = JSON.parse(grezzo) } catch { throw new Error('configurazione: il file non è JSON valido') }
  if (!c || typeof c !== 'object' || Array.isArray(c)) throw new Error('configurazione: serve un oggetto JSON')
  for (const k of Object.keys(c)) {
    if (/fortests?$/i.test(k)) throw new Error(`configurazione: "${k}" è un'opzione di prova, non si usa in produzione`)
    if (!CHIAVI_AMMESSE.includes(k)) throw new Error(`configurazione: chiave non ammessa "${k}"`)
  }
  for (const k of ['dataDir', 'keyPassphraseFile']) {
    if (typeof c[k] !== 'string' || !path.isAbsolute(c[k])) throw new Error(`configurazione: "${k}" deve essere un percorso assoluto`)
  }
  if (c.tor !== false && (typeof c.torBin !== 'string' || !path.isAbsolute(c.torBin))) throw new Error('configurazione: "torBin" deve essere il percorso assoluto di tor')
  const dati = path.resolve(c.dataDir), pass = path.resolve(c.keyPassphraseFile)
  if (pass === dati || pass.startsWith(dati + path.sep)) throw new Error('configurazione: il file della passphrase deve stare FUORI dalla cartella dati')
  controllaFilePassphrase(pass)
  return c
}

/** Il file della passphrase: esiste, non è vuoto, e (dove i permessi esistono) lo legge solo il proprietario. */
function controllaFilePassphrase(/** @type {string} */ file) {
  let st
  try { st = fs.statSync(file) } catch { throw new Error('configurazione: file della passphrase non trovato') }
  if (!st.isFile()) throw new Error('configurazione: il file della passphrase non è un file')
  if (process.platform !== 'win32' && (st.mode & 0o077) !== 0) throw new Error('configurazione: il file della passphrase è leggibile da altri utenti (serve chmod 600)')
  if (fs.readFileSync(file, 'utf8').trim().length < 20) throw new Error('configurazione: passphrase troppo corta (almeno 20 caratteri)')
}

/** @param {string[]} argv gli argomenti dopo il nome del programma */
function fileDagliArgomenti(argv) {
  const i = argv.indexOf('--config')
  if (argv.length !== 2 || i !== 0 || !argv[1]) throw new Error('uso: m4tr1x-node --config <file.json>')
  return path.resolve(argv[1])
}

/**
 * Avvia il nodo e lo tiene acceso. Un guasto (tor morto, bootstrap non completato) chiude il programma con
 * errore: lo riavvia il sistema (systemd Restart=on-failure). SIGTERM/SIGINT lo fermano in ordine.
 * @param {string[]} argv
 */
async function avvia(argv) {
  const cfg = leggiConfig(fileDagliArgomenti(argv))
  const { startNode } = require('./node')
  const nodo = await startNode({ ...cfg, onFatal: e => { process.stderr.write(`nodo fermato da un guasto: ${e.message}\n`); process.exit(1) } })
  // l'indirizzo del nodo non va nel log: sta in un file che legge solo il proprietario
  if (nodo.onion) fs.writeFileSync(path.join(/** @type {string} */ (cfg.dataDir), 'onion'), `${nodo.onion}\n`, { mode: 0o600 })
  let fermando = false
  const ferma = async () => {
    if (fermando) return
    fermando = true
    try { await nodo.close() } finally { process.exit(0) }
  }
  process.on('SIGTERM', ferma)
  process.on('SIGINT', ferma)
  return nodo
}

module.exports = { CHIAVI_AMMESSE, leggiConfig, fileDagliArgomenti, avvia }
