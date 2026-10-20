// @ts-check
'use strict'
/**
 * Vettori degli eventi (packages/protocol/vectors/eventi.json, CONGELATI): packages/protocol
 * deve dare ancora lo stesso esito su ogni evento. Lo stesso file lo legge il lato Rust
 * (app/protocol): se un esito cambia qui, cambia il contratto tra JS e Rust.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const protocol = require('../../packages/protocol')
const { rete, vettori } = require('../../packages/protocol/vectors/eventi.json')

const intero = (/** @type {any} */ ev) => ({ ...ev, sig: Array.isArray(ev.sig) ? ev.sig.join('') : ev.sig })

test('vettori degli eventi: stesso esito di packages/protocol', () => {
  assert.equal(rete, 'm4tr1x-test')
  assert.ok(vettori.length >= 32, `vettori: ${vettori.length}`)
  assert.ok(vettori.some((/** @type {any} */ v) => v.valido) && vettori.some((/** @type {any} */ v) => !v.valido))
  for (const v of vettori) assert.equal(protocol.verifyEvent(intero(v.evento), rete), v.valido, v.nome)
})

test('vettori validi: l\'id si ricalcola uguale', () => {
  for (const v of vettori.filter((/** @type {any} */ v) => v.valido)) {
    const e = v.evento
    assert.equal(protocol.eventId(e.pubkey, e.created_at, e.kind, e.tags, e.content), e.id, v.nome)
  }
})

// Nell'altro senso: un post FIRMATO nel lato Rust (app/protocol, firma_evento) passa da verifyEvent del
// JavaScript, con la chiave d'identità congelata del seme pubblico di prova (keys-vectors.test.js).
const path = require('path')
const { spawnSync } = require('child_process')
const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })
test('post firmato in Rust: il JavaScript lo verifica', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const testo = 'Nderë 🦅 "virgolette" \\ a capo\n </script>'
  const r = spawnSync('cargo', ['run', '--quiet', '--locked', '--manifest-path', path.join(__dirname, '..', '..', 'app', 'Cargo.toml'), '-p', 'm4tr1x-protocol', '--example', 'firma', '--', testo],
    { cwd: path.join(__dirname, '..', '..', 'app'), encoding: 'utf8', timeout: 900000, shell: false })
  assert.equal(r.status, 0, (r.stdout || '') + (r.stderr || ''))
  const ev = JSON.parse(r.stdout.trim().split('\n').pop() || '')
  assert.equal(ev.pubkey, '817d2b662e9c2c6e080c5fd5d9c4866a8342d194dbea7f98c1f4fc30e069bed7')
  assert.equal(ev.content, testo)
  assert.equal(protocol.verifyEvent(ev, rete), true)
  assert.equal(protocol.verifyEvent({ ...ev, content: testo + '!' }, rete), false)
})
