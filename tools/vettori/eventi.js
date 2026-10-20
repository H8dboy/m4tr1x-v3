#!/usr/bin/env node
// @ts-check
'use strict'
/**
 * Genera packages/protocol/vectors/eventi.json: eventi firmati con la chiave identità del
 * seme PUBBLICO di prova (lo stesso di tests/unit/keys-vectors.test.js: non è un segreto),
 * con l'esito che packages/protocol (JavaScript) dà a ciascuno.
 *
 * Il file si genera UNA volta e si congela: lo leggono il test JavaScript e quello Rust
 * (app/protocol), che devono dare lo stesso esito su ogni evento. Le firme BIP-340 hanno
 * una parte casuale: rigenerare cambia le firme, non gli esiti.
 *
 * Firme e chiavi da 64 byte sono salvate in due metà (lo scanner dei segreti segnala le
 * stringhe esadecimali da 128 caratteri).
 *
 * Uso: node tools/vettori/eventi.js   (sovrascrive il file: solo per aggiungere casi)
 */
const fs = require('fs')
const path = require('path')
const keys = require('../../packages/keys')
const protocol = require('../../packages/protocol')

const RETE = 'm4tr1x-test'
const seed = keys.seedFromMnemonic('abandon '.repeat(23) + 'art')
const id = keys.deriveRoleKey(seed, 'identity', 1)
const altro = keys.deriveRoleKey(seed, 'identity', 2)
const T = 1760000000
const H32 = '0123456789abcdef'.repeat(2)
const H38 = '0123456789abcdef0123456789abcdef012345'

/** @param {any} t @param {any} [k] */
const firmato = (t, k = id) => protocol._signRawEventForTest({ created_at: T, ...t }, k)
/** evento valido costruito con buildEvent (aggiunge il tag net) */
const valido = (/** @type {any} */ t) => protocol.buildEvent({ created_at: T, ...t }, id, RETE)

/** @type {Array<[string, any]>} nome, evento */
const casi = [
  ['testo semplice', valido({ kind: 1, content: 'testimonianza di prova', tags: [] })],
  ['unicode: accenti, albanese, emoji', valido({ kind: 1, content: 'Nderë e çelës — città 🦅🇦🇱 ありがとう', tags: [] })],
  ['caratteri da scappare', valido({ kind: 1, content: 'virgolette " barra \\ a capo \n tab \t ritorno \r ctrl \u0001 \u001f del \u007f sep     html </script> / fine', tags: [] })],
  ['tag vari con unicode', valido({ kind: 1, content: 'con tag', tags: [['e', 'a'.repeat(64)], ['p', 'b'.repeat(64), 'wss://esempio', 'reply'], ['t', 'città'], ['x', 'c'.repeat(64)]] })],
  ['profilo kind 0, contenuto vuoto', valido({ kind: 0, content: '', tags: [] })],
  ['kind 65535 e created_at 0', valido({ kind: 65535, content: 'limite', tags: [], created_at: 0 })],
  ['H8: 31 esadecimali dopo m4a1 (non è un conto)', valido({ kind: 1, content: `m4a1${H32.slice(1)}`, tags: [] })],
  ['H8: lettera ASCII prima (nessun confine di parola)', valido({ kind: 1, content: `xm4a1${H32}`, tags: [] })],
  ['H8: trattino basso dopo (nessun confine)', valido({ kind: 1, content: `m4a1${H32}_`, tags: [] })],
  ['H8: 33 esadecimali (nessun confine dopo il 32°)', valido({ kind: 1, content: `m4a1${H32}0`, tags: [] })],
  ['H8: tag "h8x" non è vietato', valido({ kind: 1, content: 'ok', tags: [['h8x', 'v']] })],
  // non validi: firmati davvero, ma rompono una regola
  ['H8: conto nel contenuto', firmato({ kind: 1, content: `paga m4a1${H32} grazie`, tags: [['net', RETE]] })],
  ['H8: conto in maiuscolo', firmato({ kind: 1, content: `M4A1${H32.toUpperCase()}`, tags: [['net', RETE]] })],
  ['H8: lettera non ASCII prima (in JS è un confine di parola)', firmato({ kind: 1, content: `àm4a1${H32}`, tags: [['net', RETE]] })],
  ['H8: indirizzo vecchio nel contenuto', firmato({ kind: 1, content: `H8${H38}`, tags: [['net', RETE]] })],
  ['H8: conto in un tag', firmato({ kind: 1, content: 'ok', tags: [['t', `m4a1${H32}`], ['net', RETE]] })],
  ['H8: tag "wallet" vietato', firmato({ kind: 1, content: 'ok', tags: [['wallet', 'x'], ['net', RETE]] })],
  ['H8: tag "H8Account" vietato (maiuscole)', firmato({ kind: 1, content: 'ok', tags: [['H8Account', 'x'], ['net', RETE]] })],
  ['rete sbagliata', firmato({ kind: 1, content: 'ok', tags: [['net', 'm4tr1x-main']] })],
  ['senza tag net', firmato({ kind: 1, content: 'ok', tags: [] })],
  ['due tag net', firmato({ kind: 1, content: 'ok', tags: [['net', RETE], ['net', RETE]] })],
  ['kind 65536', firmato({ kind: 65536, content: 'ok', tags: [['net', RETE]] })],
  ['tag vuoto', firmato({ kind: 1, content: 'ok', tags: [[], ['net', RETE]] })],
  ['tag con un numero', firmato({ kind: 1, content: 'ok', tags: [['t', 5], ['net', RETE]] })],
  ['created_at negativo', firmato({ kind: 1, content: 'ok', tags: [['net', RETE]], created_at: -1 })],
  ['created_at non intero', firmato({ kind: 1, content: 'ok', tags: [['net', RETE]], created_at: 1.5 })],
]
// manomissioni dopo la firma
const base = valido({ kind: 1, content: 'originale', tags: [] })
casi.push(['contenuto cambiato dopo la firma', { ...base, content: 'cambiato' }])
casi.push(['firma alterata di un bit', { ...base, sig: (parseInt(base.sig[0], 16) ^ 1).toString(16) + base.sig.slice(1) }])
casi.push(['chiave pubblica di un altro', { ...base, pubkey: altro.publicKey }])
casi.push(['id in maiuscolo', { ...base, id: base.id.toUpperCase() }])
casi.push(['contenuto non stringa', { ...base, content: 5 }])
casi.push(['firma valida di un altro evento', { ...base, sig: valido({ kind: 1, content: 'altro', tags: [] }).sig }])

const metà = (/** @type {any} */ s) => typeof s === 'string' && s.length === 128 ? [s.slice(0, 64), s.slice(64)] : s
const vettori = casi.map(([nome, ev]) => ({ nome, valido: protocol.verifyEvent(ev, RETE), evento: { ...ev, sig: metà(ev.sig) } }))

const out = path.resolve(__dirname, '..', '..', 'packages', 'protocol', 'vectors', 'eventi.json')
fs.mkdirSync(path.dirname(out), { recursive: true })
fs.writeFileSync(out, JSON.stringify({
  _nota: 'Generato da tools/vettori/eventi.js e CONGELATO. Seme pubblico di prova, rete m4tr1x-test. sig in due metà da 64 caratteri. Lo leggono tests/unit/protocol-vectors.test.js (JS) e app/protocol (Rust): stesso esito su ogni evento.',
  rete: RETE,
  vettori,
}, null, 2) + '\n')
console.log(`${vettori.length} vettori (${vettori.filter(v => v.valido).length} validi) → ${path.relative(process.cwd(), out)}`)
