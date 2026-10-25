// @ts-check
'use strict'
/**
 * I-19 — I DM escono solo come gift wrap (kind 1059). Mai kind 4, 13 o 14 pubblicati.
 * - il nodo (relay) rifiuta i kind 4, 13, 14 anche se l'evento è firmato bene e ha il tag di rete; il 1059 passa
 * - `verifyEvent` (JS) e `verifica` (Rust) rifiutano gli stessi kind
 * - parte dinamica (Rust, app/dm): dopo l'invio escono solo buste 1059, senza mittente né testo in chiaro
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const path = require('path')
const { spawnSync } = require('child_process')
const protocol = require('../../packages/protocol')
const { NET, startTestNode, wsOpen, wsExchange, testIdentity } = require('../helpers/node')

const ROOT = path.resolve(__dirname, '..', '..')
const now = () => Math.floor(Date.now() / 1000)

test('il relay rifiuta i kind 4, 13 e 14; accetta il 1059 (controllo positivo)', async () => {
  const t = await startTestNode()
  try {
    const who = testIdentity()
    const ws = await wsOpen(t.node.port)
    const invia = async (/** @type {number} */ kind) => {
      const ev = protocol._signRawEventForTest({ kind, content: 'x', tags: [['p', 'a'.repeat(64)], ['net', NET]], created_at: now() }, who)
      return (await wsExchange(ws, ['EVENT', ev], m => m[0] === 'OK' && m[1] === ev.id)).at(-1)
    }
    for (const kind of [4, 13, 14]) {
      const r = await invia(kind)
      assert.equal(r[2], false, `kind ${kind} doveva essere rifiutato`)
    }
    const buono = await invia(1059)
    assert.equal(buono[2], true, `il 1059 doveva passare: ${buono[3]}`)
    ws.close()
  } finally { await t.close() }
})

test('verifyEvent rifiuta i kind 4, 13, 14 e accetta 1 e 1059', () => {
  const who = testIdentity()
  const mk = (/** @type {number} */ kind) => protocol._signRawEventForTest({ kind, content: 'x', tags: [['net', NET]], created_at: now() }, who)
  for (const kind of [4, 13, 14]) assert.equal(protocol.verifyEvent(mk(kind), NET), false, `kind ${kind}`)
  for (const kind of [1, 1059]) assert.equal(protocol.verifyEvent(mk(kind), NET), true, `kind ${kind}`)
})

test('buildEvent non firma i kind 4, 13, 14 (e firma il 1059)', () => {
  const who = testIdentity()
  for (const kind of [4, 13, 14]) assert.throws(() => protocol.buildEvent({ kind, content: 'x', tags: [] }, who, NET), /vietato/, `kind ${kind}`)
  assert.equal(protocol.verifyEvent(protocol.buildEvent({ kind: 1059, content: 'x', tags: [] }, who, NET), NET), true)
})

const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })
function prova(/** @type {string[]} */ args) {
  const r = spawnSync('cargo', ['test', '--manifest-path', path.join(ROOT, 'app', 'Cargo.toml'), ...args],
    { cwd: path.join(ROOT, 'app'), encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  return { status: r.status, out: (r.stdout || '') + (r.stderr || '') }
}

test('parte dinamica (Rust): fuori escono solo buste 1059', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const a = prova(['-p', 'm4tr1x-protocol', '--test', 'kind_vietati'])
  assert.equal(a.status, 0, a.out.slice(-4000))
  assert.match(a.out, /i_kind_4_13_14_non_si_firmano_ne_si_accettano_e_il_1059_si \.\.\. ok/)
  const b = prova(['-p', 'm4tr1x-dm', '--test', 'inv_19_20_buste'])
  assert.equal(b.status, 0, b.out.slice(-4000))
  for (const n of ['i19_escono_solo_buste_1059_e_non_mostrano_niente', 'bruno_legge_cio_che_anna_scrive_e_anna_ha_la_sua_copia', 'testo_vuoto_o_troppo_lungo_rifiutato_prima_di_cifrare', 'si_incarta_solo_con_una_chiave_dm_e_verso_una_chiave_valida', 'il_segreto_condiviso_lo_calcola_solo_la_chiave_dm'])
    assert.match(b.out, new RegExp(`${n} \\.\\.\\. ok`), `manca o fallisce la prova ${n}`)
})
