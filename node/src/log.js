// @ts-check
'use strict'
/**
 * Log del nodo (I-18): il codice non passa mai indirizzi al log, e come seconda
 * difesa ogni riga viene ripulita da qualunque cosa somigli a un IPv4/IPv6.
 */
const IPV4 = /\b(?:\d{1,3}\.){3}\d{1,3}\b/g
const IPV6 = /\b(?:[0-9a-f]{1,4}:){2,7}[0-9a-f]{0,4}\b|::1\b|::ffff:[0-9a-f.:]*/gi

/** @param {string} s */
function scrub(s) {
  return String(s).replace(IPV4, '[ip]').replace(IPV6, '[ip]')
}

/** @param {((line: string) => void) | undefined} sink */
function makeLogger(sink) {
  const out = sink || (line => process.stdout.write(`${new Date().toISOString()} ${line}\n`))
  return (/** @type {string} */ msg) => out(scrub(msg))
}

module.exports = { makeLogger, scrub }
