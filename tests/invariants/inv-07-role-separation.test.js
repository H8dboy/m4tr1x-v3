// @ts-check
'use strict'
/**
 * I-07 — Ruoli separati: una chiave firma solo ciò che il suo ruolo può firmare,
 * e una firma fatta con la chiave di un ruolo non vale dove serve un altro ruolo.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const keys = require('../../packages/keys')
const protocol = require('../../packages/protocol')

const NET = 'm4tr1x-test'
const seed = keys.seedFromMnemonic(keys.generateMnemonic())
const identity = keys.deriveRoleKey(seed, 'identity')
const wallet = keys.deriveRoleKey(seed, 'wallet')
const dm = keys.deriveRoleKey(seed, 'dm')
const revoke = keys.deriveRoleKey(seed, 'revoke')
const spend = new TextEncoder().encode(JSON.stringify({ a: 'esercente', importo: 100, seq: 1 }))

test('ogni ruolo ha una chiave diversa (stesso seme)', () => {
  const pubs = [identity, wallet, dm, revoke].map(k => k.publicKey)
  assert.equal(new Set(pubs).size, 4)
})

test('la chiave identità non può autorizzare spese H8', () => {
  assert.throws(() => protocol.signDomain(identity, 'h8/spend', NET, spend), /ruolo/)
})

test('la chiave wallet non può firmare eventi Nostr', () => {
  assert.throws(() => protocol.buildEvent({ kind: 1, content: 'ciao', tags: [] }, wallet, NET), /ruolo/)
})

test('le chiavi DM e revoca non firmano post', () => {
  assert.throws(() => protocol.buildEvent({ kind: 1, content: 'x', tags: [] }, dm, NET), /ruolo/)
  assert.throws(() => protocol.buildEvent({ kind: 1, content: 'x', tags: [] }, revoke, NET), /ruolo/)
})

test('una spesa firmata dal wallet non vale come firma dell\'identità e viceversa', () => {
  const sig = protocol.signDomain(wallet, 'h8/spend', NET, spend)
  assert.ok(protocol.verifyDomain(wallet.publicKey, sig, 'h8/spend', NET, spend))
  assert.ok(!protocol.verifyDomain(identity.publicKey, sig, 'h8/spend', NET, spend))
})

test('solo la chiave di revoca firma una revoca', () => {
  const payload = new TextEncoder().encode(identity.publicKey)
  assert.throws(() => protocol.signDomain(identity, 'identity/revoke', NET, payload), /ruolo/)
  const sig = protocol.signDomain(revoke, 'identity/revoke', NET, payload)
  assert.ok(protocol.verifyDomain(revoke.publicKey, sig, 'identity/revoke', NET, payload))
})
