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
