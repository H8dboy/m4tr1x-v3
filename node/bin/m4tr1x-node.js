#!/usr/bin/env node
// @ts-check
'use strict'
/**
 * Programma del nodo M4TR1X per il servizio di sistema.
 * Uso: m4tr1x-node --config /etc/m4tr1x/node.json
 * Esce con 1 su configurazione non valida, avvio fallito o guasto (lo riavvia il sistema).
 */
const { avvia } = require('../src/cli')

avvia(process.argv.slice(2)).catch(e => {
  process.stderr.write(`${String(/** @type {Error} */ (e).message || e)}\n`)
  process.exit(1)
})
