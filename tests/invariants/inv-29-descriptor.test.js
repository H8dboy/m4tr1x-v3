// @ts-check
'use strict'
/**
 * I-29 — I descrittori della directory contengono solo un onion v3, sono firmati
 * dalla chiave del nodo e scadono entro 7 giorni. Nessun IP, da nessuna parte.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const keys = require('../../packages/keys')
const protocol = require('../../packages/protocol')

const NET = 'm4tr1x-test'
const ONION = 'b'.repeat(56) + '.onion'
const nodeKey = keys.generateNodeKey()
const now = () => Math.floor(Date.now() / 1000)

test('descrittore corretto: valido', () => {
  const d = protocol.buildDescriptor(nodeKey, NET, { onion: ONION, roles: ['user'], version: '3.0.0', ttlSec: 3 * 86400, powBits: 0 })
  assert.deepEqual(protocol.verifyDescriptor(d, NET, now()), { ok: true })
})

test('onion non v3 o IP al posto dell\'onion: rifiutato già in costruzione', () => {
  for (const onion of ['x.onion', '127.0.0.1', '203.0.113.8', 'nodo.example.com', 'b'.repeat(16) + '.onion']) {
    assert.throws(() => protocol.buildDescriptor(nodeKey, NET, { onion, roles: ['user'], version: '3', ttlSec: 3600, powBits: 0 }), /onion/)
  }
})

test('IP nascosto nel contenuto o in un tag: rifiutato in verifica', () => {
  const raw = (/** @type {any} */ extra) => protocol._signRawEventForTest({
    kind: protocol.DESCRIPTOR_KIND, created_at: now(),
    content: extra.content ?? JSON.stringify({ roles: ['user'] }),
    tags: [['d', 'm4tr1x-node'], ['onion', ONION], ['expiration', String(now() + 3600)], ['net', NET], ...(extra.tags || [])],
  }, nodeKey)
  const ip = '198.51.100.4'   // indirizzo di documentazione (RFC 5737)
  assert.equal(protocol.verifyDescriptor(raw({ content: JSON.stringify({ roles: ['user'], fallback: ip }) }), NET, now()).ok, false)
  assert.equal(protocol.verifyDescriptor(raw({ tags: [['r', 'http://' + ip + ':8080']] }), NET, now()).ok, false)
})

test('firma di un\'altra chiave o descrittore alterato: rifiutato', () => {
  const d = protocol.buildDescriptor(nodeKey, NET, { onion: ONION, roles: ['user'], version: '3', ttlSec: 3600, powBits: 0 })
  assert.equal(protocol.verifyDescriptor({ ...d, pubkey: keys.generateNodeKey().publicKey }, NET, now()).ok, false)
  assert.equal(protocol.verifyDescriptor({ ...d, content: d.content.replace('user', 'hub') }, NET, now()).ok, false)
})

test('la chiave identità di un utente non può firmare un descrittore, la chiave nodo non può firmare post', () => {
  const identity = keys.deriveRoleKey(keys.seedFromMnemonic(keys.generateMnemonic()), 'identity')
  assert.throws(() => protocol.buildDescriptor(identity, NET, { onion: ONION, roles: ['user'], version: '3', ttlSec: 3600, powBits: 0 }), /ruolo|nodo/)
  assert.throws(() => protocol.buildEvent({ kind: 1, content: 'post', tags: [] }, nodeKey, NET), /nodo|ruolo/)
})

test('scadenza oltre 7 giorni: rifiutata', () => {
  assert.throws(() => protocol.buildDescriptor(nodeKey, NET, { onion: ONION, roles: ['user'], version: '3', ttlSec: 8 * 86400, powBits: 0 }), /7 giorni/)
  const forged = protocol._signRawEventForTest({
    kind: protocol.DESCRIPTOR_KIND, created_at: now(), content: JSON.stringify({ roles: ['user'] }),
    tags: [['d', 'm4tr1x-node'], ['onion', ONION], ['expiration', String(now() + 30 * 86400)], ['net', NET]],
  }, nodeKey)
  assert.equal(protocol.verifyDescriptor(forged, NET, now()).ok, false)
})

test('ruolo sconosciuto: rifiutato', () => {
  assert.throws(() => protocol.buildDescriptor(nodeKey, NET, { onion: ONION, roles: ['root'], version: '3', ttlSec: 3600, powBits: 0 }), /ruolo/)
})
