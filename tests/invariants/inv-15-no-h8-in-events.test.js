// @ts-check
'use strict'
/**
 * I-15 — Nessun indirizzo o conto H8 in un evento Nostr pubblicato.
 * Il costruttore di eventi rifiuta tag e contenuti che collegano l'identità
 * pubblica a un conto H8 (ADR-0008: identità e denaro separati).
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const keys = require('../../packages/keys')
const protocol = require('../../packages/protocol')

const NET = 'm4tr1x-test'
const identity = keys.deriveRoleKey(keys.seedFromMnemonic(keys.generateMnemonic()), 'identity')
const ACCOUNT = protocol.newAccountId()
const LEGACY = 'H8' + 'a'.repeat(38)

for (const name of ['h8', 'h8host', 'wallet', 'account', 'h8account']) {
  test(`tag "${name}" rifiutato`, () => {
    assert.throws(() => protocol.buildEvent({ kind: 1, content: 'x', tags: [[name, ACCOUNT]] }, identity, NET), /H8/)
  })
}

test('id di conto H8 nel contenuto rifiutato', () => {
  assert.throws(() => protocol.buildEvent({ kind: 1, content: `mandami mance su ${ACCOUNT}`, tags: [] }, identity, NET), /H8/)
})

test('indirizzo H8 vecchio formato in un tag qualsiasi rifiutato', () => {
  assert.throws(() => protocol.buildEvent({ kind: 1, content: 'x', tags: [['t', LEGACY]] }, identity, NET), /H8/)
})

test('verifyEvent rifiuta anche eventi ricevuti con dati H8', () => {
  const ev = protocol._signRawEventForTest({ kind: 1, content: ACCOUNT, tags: [['net', NET]], created_at: 1 }, identity)
  assert.ok(!protocol.verifyEvent(ev, NET))
})

test('un evento normale passa', () => {
  const ev = protocol.buildEvent({ kind: 1, content: 'Testimonianza da Albania, ore 21', tags: [['t', 'diaspora']] }, identity, NET)
  assert.ok(protocol.verifyEvent(ev, NET))
})
