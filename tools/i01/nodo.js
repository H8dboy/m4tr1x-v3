#!/usr/bin/env node
// @ts-check
'use strict'
/**
 * I-01 dinamica: nodo VERO con il suo onion per l'app sotto prova (gira come utente del runner, fuori dalle
 * regole di rete dell'utente dell'app). Aspetta la pubblicazione confermata del descrittore, pubblica un post,
 * scrive l'onion in <file> (0600: l'onion non va nel log) e resta acceso finché non riceve SIGTERM.
 *
 * Uso: M4TR1X_TOR_BIN=/usr/bin/tor node tools/i01/nodo.js <file dell'onion>
 */
const fs = require('fs')
const protocol = require('../../packages/protocol')
const { NET, startTestNode, wsOpen, wsExchange, testIdentity } = require('../../tests/helpers/node')

/** Testo del post che la prova cerca nel feed dell'app. */
const TESTO = 'I-01: post letto dall\'app vera via Tor, senza un pacchetto fuori'

async function main() {
  const file = process.argv[2]
  const tor = process.env.M4TR1X_TOR_BIN
  if (!file || !tor) throw new Error('uso: M4TR1X_TOR_BIN=<tor> nodo.js <file dell\'onion>')
  const autore = testIdentity()
  const n = await startTestNode({ uploaders: [autore.publicKey], tor: true, torBin: tor })
  const annuncio = await n.node.announceDecision
  if (annuncio !== 'confermata') throw new Error(`fase "pubblicazione": descrittore non confermato (${annuncio})`)
  const ws = await wsOpen(n.node.port)
  const ok = await wsExchange(ws, ['EVENT', protocol.buildEvent({ kind: 1, content: TESTO, tags: [] }, autore, NET)], m => m[0] === 'OK')
  ws.close()
  if (ok.at(-1)[2] !== true) throw new Error(`post rifiutato dal nodo: ${JSON.stringify(ok.at(-1))}`)
  fs.writeFileSync(file, String(n.node.onion), { mode: 0o600 })
  console.log('nodo pronto: descrittore confermato, post pubblicato')
  const ferma = async () => { try { await n.node.close() } finally { process.exit(0) } }
  process.on('SIGTERM', ferma)
  process.on('SIGINT', ferma)
}

module.exports = { TESTO }
if (require.main === module) main().catch(e => { console.error(String(e && e.stack || e)); process.exit(1) })
