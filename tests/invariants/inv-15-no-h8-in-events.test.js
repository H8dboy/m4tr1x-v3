// @ts-check
'use strict'
/**
 * I-15 — Nessun indirizzo o conto H8 in un evento Nostr pubblicato.
 * Il costruttore di eventi rifiuta tag e contenuti che collegano l'identità
 * pubblica a un conto H8 (ADR-0008: identità e denaro separati).
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const path = require('path')
const { spawnSync } = require('child_process')
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

// Lato Rust dell'app (app/protocol, F4): stessa regola sugli eventi ricevuti, stessi esiti del
// JavaScript sui vettori congelati (packages/protocol/vectors/eventi.json), confini di parola compresi.
const ROOT = path.resolve(__dirname, '..', '..')
const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })
test('lato Rust (app/protocol): eventi con dati H8 rifiutati come in JavaScript', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(ROOT, 'app', 'Cargo.toml'), '-p', 'm4tr1x-protocol'],
    { cwd: path.join(ROOT, 'app'), encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  for (const t of ['confini_di_parola_come_il_javascript', 'stesso_esito_del_javascript_su_ogni_vettore', 'i_rifiuti_hanno_il_motivo_giusto', 'dati_h8_rifiutati_prima_di_firmare'])
    assert.match(out, new RegExp(`${t} \\.\\.\\. ok`), `manca o fallisce la prova ${t}`)
})
