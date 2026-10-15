// @ts-check
'use strict'
/**
 * Verifica di firme in formato minisign (Ed25519), senza dipendenze esterne.
 *
 * Formato (https://jedisct1.github.io/minisign/):
 *   chiave pubblica: base64( "Ed" | key_id[8] | pk[32] )
 *   firma:           riga 2 = base64( alg[2] | key_id[8] | sig[64] )
 *                    alg "ED" = firma su BLAKE2b-512(file)   (default di minisign)
 *                    alg "Ed" = firma sul file intero          (legacy)
 *                    riga 3 = "trusted comment: <testo>"
 *                    riga 4 = base64( firma globale su sig[64] || <testo> )
 */
const crypto = require('crypto')

const SPKI_ED25519_PREFIX = Buffer.from('302a300506032b6570032100', 'hex')

/** @param {Buffer} raw32 */
function ed25519PublicKey(raw32) {
  return crypto.createPublicKey({ key: Buffer.concat([SPKI_ED25519_PREFIX, raw32]), format: 'der', type: 'spki' })
}

/** Ultima riga non di commento di un file di chiave o firma. @param {string} text */
function dataLines(text) {
  return text.replace(/\r\n/g, '\n').split('\n').filter(l => l.length > 0)
}

/**
 * @param {string} pubText contenuto del file .pub
 * @returns {{ keyId: Buffer, key: crypto.KeyObject }}
 */
function parsePublicKey(pubText) {
  const line = dataLines(pubText).find(l => !l.startsWith('untrusted comment:'))
  if (!line) throw new Error('chiave pubblica vuota')
  const raw = Buffer.from(line.trim(), 'base64')
  if (raw.length !== 42 || raw.subarray(0, 2).toString('latin1') !== 'Ed') throw new Error('chiave pubblica non minisign Ed25519')
  return { keyId: raw.subarray(2, 10), key: ed25519PublicKey(raw.subarray(10, 42)) }
}

/**
 * Verifica `message` con la firma minisign `sigText` e la chiave `pubText`.
 * Ritorna il trusted comment se tutto è valido, altrimenti lancia.
 * @param {Buffer} message
 * @param {string} sigText
 * @param {string} pubText
 * @returns {string}
 */
function verify(message, sigText, pubText) {
  const pub = parsePublicKey(pubText)
  const lines = dataLines(sigText)
  if (lines.length < 4 || !lines[0].startsWith('untrusted comment:') || !lines[2].startsWith('trusted comment: '))
    throw new Error('file di firma malformato')
  const sigRaw = Buffer.from(lines[1].trim(), 'base64')
  if (sigRaw.length !== 74) throw new Error('firma di lunghezza errata')
  const alg = sigRaw.subarray(0, 2).toString('latin1')
  const keyId = sigRaw.subarray(2, 10)
  const sig = sigRaw.subarray(10, 74)
  if (!keyId.equals(pub.keyId)) throw new Error('firma fatta con un\'altra chiave')
  let signed
  if (alg === 'ED') signed = crypto.createHash('blake2b512').update(message).digest()
  else if (alg === 'Ed') signed = message
  else throw new Error('algoritmo di firma sconosciuto')
  if (!crypto.verify(null, signed, pub.key, sig)) throw new Error('firma non valida')
  const trusted = lines[2].slice('trusted comment: '.length)
  const globalSig = Buffer.from(lines[3].trim(), 'base64')
  if (globalSig.length !== 64 || !crypto.verify(null, Buffer.concat([sig, Buffer.from(trusted, 'utf8')]), pub.key, globalSig))
    throw new Error('trusted comment non valido')
  return trusted
}

module.exports = { verify, parsePublicKey }
