#!/usr/bin/env node
// @ts-check
'use strict'
/**
 * I-01 dinamica: legge la riga di `rete.sh conta` e decide. Passa SOLO se:
 *   - nessun pacchetto dell'utente dell'app fuori dalle regole (IPv4 e IPv6 a 0);
 *   - pacchetti verso i relay > 0 e sul loopback > 0: tor ha lavorato davvero e l'app ha parlato con lui
 *     (senza, contatori a 0 vorrebbero dire "non è successo niente", non "niente è uscito").
 * Uso: rete.sh conta | node tools/i01/verifica.js
 */

/** @param {string} riga @returns {{ ok: boolean, motivi: string[], valori: Record<string, number> }} */
function verifica(riga) {
  /** @type {Record<string, number>} */ const valori = {}
  for (const k of ['violazioni_ipv4', 'violazioni_ipv6', 'pacchetti_verso_relay', 'pacchetti_loopback']) {
    const m = new RegExp(`\\b${k}=(\\d+)\\b`).exec(riga)
    valori[k] = m ? Number(m[1]) : NaN
  }
  const motivi = []
  for (const [k, v] of Object.entries(valori)) if (!Number.isInteger(v)) motivi.push(`${k} non letto`)
  if (valori.violazioni_ipv4 > 0) motivi.push(`${valori.violazioni_ipv4} pacchetti IPv4 fuori da Tor`)
  if (valori.violazioni_ipv6 > 0) motivi.push(`${valori.violazioni_ipv6} pacchetti IPv6 fuori da Tor`)
  if (valori.pacchetti_verso_relay === 0) motivi.push('nessun pacchetto verso i relay: tor non ha lavorato, la prova non dimostra niente')
  if (valori.pacchetti_loopback === 0) motivi.push('nessun pacchetto sul loopback: l\'app non ha parlato con il suo tor')
  return { ok: motivi.length === 0, motivi, valori }
}

module.exports = { verifica }

if (require.main === module) {
  const riga = require('fs').readFileSync(0, 'utf8')
  const r = verifica(riga)
  console.log(`I-01: ${JSON.stringify(r.valori)}`)
  if (!r.ok) { console.error(`I-01 ROSSA: ${r.motivi.join('; ')}`); process.exit(1) }
  console.log('I-01 verde: nessun pacchetto dell\'app fuori da Tor, e tor ha lavorato davvero')
}
