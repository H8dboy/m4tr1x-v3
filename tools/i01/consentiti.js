#!/usr/bin/env node
// @ts-check
'use strict'
/**
 * I-01 dinamica (decisione del fondatore, 21/10/2026, opzione A): le SOLE destinazioni fuori dal loopback
 * che l'utente dell'app può raggiungere sono i relay Tor. Tor gira con lo stesso utente dell'app (è suo
 * figlio, ADR-0016), quindi iptables non li distingue: si ammettono solo indirizzi e porte che tor stesso
 * usa, ricavati da fonti verificate, mai scritti a mano:
 *   - autorità e directory di riserva scritte DENTRO il binario di tor verificato (le usa al primo avvio);
 *   - relay del consenso attuale (righe `r` di cached-microdesc-consensus, firmato dalle autorità).
 * Limite dichiarato in I-01: un collegamento dell'app verso un indirizzo che è anche un relay non si vede.
 *
 * Uso: node tools/i01/consentiti.js <tor> <cached-microdesc-consensus>  → righe "add m4relay IP,tcp:PORTA"
 */
const fs = require('fs')

const IP4 = String.raw`(?:25[0-5]|2[0-4]\d|1?\d?\d)(?:\.(?:25[0-5]|2[0-4]\d|1?\d?\d)){3}`

/**
 * Autorità e directory di riserva dal binario di tor (letto come byte).
 * @param {Buffer} bin @returns {{ riserva: string[], autorita: string[] }} voci "IP:PORTA"
 */
function daBinario(bin) {
  const t = bin.toString('latin1')
  const riserva = [...t.matchAll(new RegExp(`(?<![\\d.])(${IP4}) orport=(\\d{1,5}) id=[0-9A-F]{40}`, 'g'))].map(m => `${m[1]}:${m[2]}`)
  /** @type {string[]} */ const autorita = []
  for (const m of t.matchAll(new RegExp(`([A-Za-z0-9]+) orport=(\\d{1,5}) [^\\0]*?(${IP4}):(\\d{1,5}) [0-9A-F]{4}( ?[0-9A-F]{4}){9}`, 'g'))) {
    autorita.push(`${m[3]}:${m[2]}`, `${m[3]}:${m[4]}`)   // porta OR e porta di directory
  }
  return { riserva, autorita }
}

/**
 * Relay del consenso microdescrittori: "r <nome> <id> <data> <ora> <IP> <ORPort> <DirPort>".
 * @param {string} consenso @returns {string[]} voci "IP:PORTA" (ORPort, e DirPort se non 0)
 */
function daConsenso(consenso) {
  /** @type {string[]} */ const out = []
  for (const m of consenso.matchAll(new RegExp(`^r \\S+ \\S+ \\d{4}-\\d\\d-\\d\\d \\d\\d:\\d\\d:\\d\\d (${IP4}) (\\d{1,5}) (\\d{1,5})$`, 'gm'))) {
    out.push(`${m[1]}:${m[2]}`)
    if (m[3] !== '0') out.push(`${m[1]}:${m[3]}`)
  }
  return out
}

/**
 * Elenco unico e ordinato; si ferma se una fonte è sospettosamente vuota (un elenco vuoto o quasi
 * farebbe sembrare "tutto bloccato" una prova che non ha visto tor lavorare).
 * @param {Buffer} bin @param {string} consenso
 */
function consentiti(bin, consenso) {
  const { riserva, autorita } = daBinario(bin)
  const relay = daConsenso(consenso)
  if (autorita.length < 2 * 7) throw new Error(`autorità trovate nel binario: ${autorita.length / 2} (attese almeno 7)`)
  if (riserva.length < 100) throw new Error(`directory di riserva trovate nel binario: ${riserva.length} (attese almeno 100)`)
  if (relay.length < 1000) throw new Error(`relay nel consenso: ${relay.length} (attesi almeno 1000)`)
  const tutte = [...new Set([...autorita, ...riserva, ...relay])].sort()
  for (const v of tutte) {
    const porta = Number(v.split(':')[1])
    if (!(porta > 0 && porta < 65536)) throw new Error(`porta non valida: ${v}`)
  }
  return { tutte, conti: { autorita: autorita.length / 2, riserva: riserva.length, relay: relay.length } }
}

module.exports = { daBinario, daConsenso, consentiti }

if (require.main === module) {
  const [bin, cons] = process.argv.slice(2)
  if (!bin || !cons) { console.error('uso: consentiti.js <tor> <cached-microdesc-consensus>'); process.exit(2) }
  const { tutte, conti } = consentiti(fs.readFileSync(bin), fs.readFileSync(cons, 'utf8'))
  process.stdout.write(tutte.map(v => { const [ip, p] = v.split(':'); return `add m4relay ${ip},tcp:${p}\n` }).join(''))
  console.error(`consentiti: ${tutte.length} destinazioni (autorità ${conti.autorita}, riserva ${conti.riserva}, relay ${conti.relay})`)
}
