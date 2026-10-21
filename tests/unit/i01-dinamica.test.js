// @ts-check
'use strict'
/**
 * Strumenti della prova dinamica di I-01 (tools/i01/): elenco dei relay consentiti ricavato da fonti
 * verificate, e decisione sui contatori. La prova vera gira nel job CI i01-rete (Linux, root).
 * Indirizzi: SOLO di documentazione (RFC 5737).
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const { daBinario, daConsenso, consentiti } = require('../../tools/i01/consentiti')
const { verifica } = require('../../tools/i01/verifica')

const DOC = ['192.0.2', '198.51.100', '203.0.113']
const ip = (/** @type {number} */ i) => `${DOC[i % 3]}.${(i % 250) + 1}`
const ID = 'A'.repeat(40)

/** Binario finto con `na` autorità e `nr` directory di riserva, come stanno dentro tor (stringhe con \0). */
function binario(na = 9, nr = 200) {
  const parti = ['codice\0']
  for (let i = 0; i < na; i++) parti.push(`aut${i} orport=${9000 + i} v3ident=${ID} ${ip(i)}:${8000 + i} ${'ABCD '.repeat(9)}ABCD\0`)
  for (let i = 0; i < nr; i++) parti.push(`${ip(i)} orport=${443 + (i % 3)} id=${ID}\0`)
  return Buffer.from(parti.join('\0\0'), 'latin1')
}

/** Consenso finto con `n` righe r. */
function consenso(n = 1200) {
  const righe = ['network-status-version 3 microdesc']
  for (let i = 0; i < n; i++) righe.push(`r rel${i} AAAA 2026-10-21 08:00:00 ${ip(i)} ${9001 + (i % 50)} ${i % 4 ? 0 : 9030}`, 'm abc')
  return righe.join('\n') + '\n'
}

test('consentiti: autorità (porta OR e di directory), riserva e relay, senza doppioni', () => {
  const b = daBinario(binario())
  assert.equal(b.autorita.length, 18)
  assert.ok(b.autorita.includes(`${ip(0)}:9000`) && b.autorita.includes(`${ip(0)}:8000`))
  assert.equal(b.riserva.length, 200)
  const r = daConsenso(consenso())
  assert.ok(r.includes(`${ip(0)}:9001`) && r.includes(`${ip(0)}:9030`), 'ORPort e DirPort (se non 0)')
  assert.ok(!r.some(v => v.endsWith(':0')), 'DirPort 0 non è una destinazione')
  const { tutte } = consentiti(binario(), consenso())
  assert.equal(new Set(tutte).size, tutte.length)
})

test('consentiti: una fonte quasi vuota ferma la prova (non diventa "tutto bloccato")', () => {
  assert.throws(() => consentiti(binario(3), consenso()), /autorità/)
  assert.throws(() => consentiti(binario(9, 20), consenso()), /riserva/)
  assert.throws(() => consentiti(binario(), consenso(50)), /relay/)
})

test('consentiti: righe malformate o IP non validi non entrano', () => {
  const r = daConsenso('r x AAAA 2026-10-21 08:00:00 999.0.2.1 9001 0\nr x AAAA 2026-10-21 08:00:00 192.0.2.7 9001 0 extra\nrx AAAA\n')
  assert.deepEqual(r, [])
  const b = daBinario(Buffer.from(`1192.0.2.5 orport=1 id=${ID}\0`, 'latin1'))
  assert.deepEqual(b.riserva, [], 'un IP attaccato ad altre cifre non va preso a metà')
})

test('verifica: verde solo con 0 violazioni E tor che ha lavorato davvero', () => {
  assert.equal(verifica('violazioni_ipv4=0 violazioni_ipv6=0 pacchetti_verso_relay=812 pacchetti_loopback=4410').ok, true)
  assert.match(verifica('violazioni_ipv4=3 violazioni_ipv6=0 pacchetti_verso_relay=812 pacchetti_loopback=10').motivi.join(), /IPv4/)
  assert.match(verifica('violazioni_ipv4=0 violazioni_ipv6=1 pacchetti_verso_relay=812 pacchetti_loopback=10').motivi.join(), /IPv6/)
  assert.match(verifica('violazioni_ipv4=0 violazioni_ipv6=0 pacchetti_verso_relay=0 pacchetti_loopback=10').motivi.join(), /non ha lavorato/)
  assert.match(verifica('violazioni_ipv4=0 violazioni_ipv6=0 pacchetti_verso_relay=5 pacchetti_loopback=0').motivi.join(), /loopback/)
  assert.equal(verifica('violazioni_ipv4=? violazioni_ipv6=0 pacchetti_verso_relay=5 pacchetti_loopback=5').ok, false, 'contatore illeggibile = rosso')
  assert.equal(verifica('').ok, false)
})

test('la prova dinamica gira davvero in CI: job i01-rete con controllo positivo, app come utente dedicato, verifica', () => {
  const ci = fs.readFileSync(path.resolve(__dirname, '..', '..', '.github', 'workflows', 'ci.yml'), 'utf8')
  const job = /\n  i01-rete:\n([\s\S]*?)(?=\n  [a-z][\w-]*:\n|$)/.exec(ci)
  assert.ok(job, 'job i01-rete assente dalla CI')
  const passi = job[1]
  for (const [cosa, re] of /** @type {[string, RegExp][]} */ ([
    ['hash di tauri-driver', /tauri_driver\.crate_sha256[\s\S]*sha256sum -c -/],
    ['versione di WebKitWebDriver nel log', /dpkg-query -W[^\n]*webkit2gtk-driver/],
    ['bundle tor del lock verificato', /tor-linux\.lock\.json[\s\S]*bundle_sha256[\s\S]*sha256sum -c -/],
    ['regole di rete', /rete\.sh prepara/],
    ['controllo positivo', /rete\.sh controllo/],
    ['app come utente dedicato', /sudo -u m4app env -i [^\n]*\\\n\s*[^\n]*\/opt\/m4i01\/tauri-driver /],
    ['guida con WebDriver', /node tools\/i01\/prova\.js/],
    ['contatori verificati', /rete\.sh conta[^\n]*\|[^\n]*node tools\/i01\/verifica\.js/],
  ])) assert.match(passi, re, `job i01-rete senza: ${cosa}`)
  // il controllo positivo viene PRIMA della prova, la verifica DOPO
  assert.ok(passi.indexOf('rete.sh controllo') < passi.indexOf('tools/i01/prova.js'), 'controllo positivo dopo la prova')
  assert.ok(passi.indexOf('tools/i01/prova.js') < passi.indexOf('tools/i01/verifica.js'), 'verifica prima della prova')
})
