// @ts-check
'use strict'
/**
 * I-08 — Ogni firma porta un'etichetta di dominio e l'identificativo di rete:
 * una firma valida in rete di prova non vale in rete principale, e una firma
 * fatta per uno scopo non vale per un altro.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const keys = require('../../packages/keys')
const protocol = require('../../packages/protocol')

const seed = keys.seedFromMnemonic(keys.generateMnemonic())
const identity = keys.deriveRoleKey(seed, 'identity')
const wallet = keys.deriveRoleKey(seed, 'wallet')
const payload = new TextEncoder().encode('{"importo":100,"seq":7}')

test('firma di rete di prova rifiutata in rete principale', () => {
  const sig = protocol.signDomain(wallet, 'h8/spend', 'm4tr1x-test', payload)
  assert.ok(protocol.verifyDomain(wallet.publicKey, sig, 'h8/spend', 'm4tr1x-test', payload))
  assert.ok(!protocol.verifyDomain(wallet.publicKey, sig, 'h8/spend', 'm4tr1x-main', payload))
})

test('firma di un dominio rifiutata in un altro dominio', () => {
  const sig = protocol.signDomain(wallet, 'h8/spend', 'm4tr1x-test', payload)
  assert.ok(!protocol.verifyDomain(wallet.publicKey, sig, 'h8/rekey', 'm4tr1x-test', payload))
})

test('reti e domini sconosciuti rifiutati', () => {
  assert.throws(() => protocol.signDomain(wallet, 'h8/spend', 'altra-rete', payload), /rete/)
  assert.throws(() => protocol.signDomain(wallet, 'h8/inventato', 'm4tr1x-test', payload), /dominio/)
})

test('evento di rete di prova rifiutato in rete principale; evento senza rete rifiutato', () => {
  const ev = protocol.buildEvent({ kind: 1, content: 'ciao', tags: [] }, identity, 'm4tr1x-test')
  assert.ok(protocol.verifyEvent(ev, 'm4tr1x-test'))
  assert.ok(!protocol.verifyEvent(ev, 'm4tr1x-main'))
  // stesso contenuto ma firmato senza tag di rete (evento Nostr "nudo")
  const naked = protocol._signRawEventForTest({ kind: 1, content: 'ciao', tags: [], created_at: ev.created_at }, identity)
  assert.ok(!protocol.verifyEvent(naked, 'm4tr1x-test'))
})

test('evento alterato dopo la firma rifiutato', () => {
  const ev = protocol.buildEvent({ kind: 1, content: 'originale', tags: [] }, identity, 'm4tr1x-test')
  assert.ok(!protocol.verifyEvent({ ...ev, content: 'cambiato' }, 'm4tr1x-test'))
})
