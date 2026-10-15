// @ts-check
'use strict'
/**
 * I-26 — Un input malformato non fa cadere il nodo; memoria con un tetto.
 * Fuzzing del relay (frame casuali, JSON strani, filtri ostili, eventi enormi)
 * e delle API HTTP; alla fine il nodo deve rispondere e la memoria restare sotto soglia.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const crypto = require('crypto')
const { startTestNode, request, wsOpen } = require('../helpers/node')

const DURATION_MS = Number(process.env.FUZZ_MS || 6000)

function randomFrame() {
  const r = Math.random()
  if (r < 0.15) return crypto.randomBytes(1 + Math.floor(Math.random() * 512))
  if (r < 0.25) return '{}'
  if (r < 0.35) return '[]'
  if (r < 0.45) return JSON.stringify(['REQ'])
  if (r < 0.55) return JSON.stringify(['REQ', 'x'.repeat(5000), { kinds: 'a' }])
  if (r < 0.65) return JSON.stringify(['REQ', 's', ...Array(50).fill({ authors: Array(2000).fill('ab') })])
  if (r < 0.75) return JSON.stringify(['EVENT', { id: 'x', tags: 'no', kind: -1 }])
  if (r < 0.85) return JSON.stringify(['EVENT', { content: 'x'.repeat(200000) }])
  if (r < 0.92) return JSON.stringify(['CLOSE', { a: 1 }])
  return '['.repeat(5000)
}

test('fuzzing di relay e API: il nodo resta vivo, memoria sotto soglia', async () => {
  const t = await startTestNode()
  const rssStart = process.memoryUsage().rss
  try {
    const end = Date.now() + DURATION_MS
    let sent = 0
    while (Date.now() < end) {
      const ws = await wsOpen(t.node.port).catch(() => null)
      if (ws) {
        ws.on('error', () => {})
        for (let i = 0; i < 40 && ws.readyState === ws.OPEN; i++) { try { ws.send(randomFrame()); sent++ } catch {} }
        ws.close()
      }
      await request(t.node.port, '/blob', { method: 'PUT', body: crypto.randomBytes(2048), headers: { Authorization: 'Nostr ' + crypto.randomBytes(40).toString('base64') } }).catch(() => {})
      await request(t.node.port, '/blob/' + crypto.randomBytes(40).toString('hex')).catch(() => {})
      await request(t.node.port, '/' + '%'.repeat(100)).catch(() => {})
    }
    assert.ok(sent > 100, `troppi pochi frame inviati: ${sent}`)
    assert.equal((await request(t.node.port, '/health')).status, 200, 'il nodo non risponde più')
    const growthMB = (process.memoryUsage().rss - rssStart) / 1048576
    assert.ok(growthMB < 300, `memoria cresciuta di ${growthMB.toFixed(0)} MB`)
  } finally { await t.close() }
})
