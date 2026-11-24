// @ts-check
'use strict'
/**
 * Coda della replica dei file tra peer (node/src/coda-replica.js, RIPRESA 91 f):
 *   pochi eventi insieme, tetto ai file in attesa, un evento lasciato perdere al primo file che non arriva.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const { creaCoda, INSIEME, MASSIMO } = require('../../node/src/coda-replica')

const sha = (/** @type {number} */ i) => i.toString(16).padStart(64, '0')
const pausa = (/** @type {number} */ ms) => new Promise(r => setTimeout(r, ms))

/** Una coda con uno scaricatore finto: `esito(sha)` decide se il file arriva; conta chiamate e massimo parallelo. */
function prova(/** @type {(s: string) => boolean} */ esito, /** @type {any} */ o = {}) {
  /** @type {string[]} */ const chiesti = []
  /** @type {string[]} */ const logs = []
  /** @type {Set<string>} */ const presenti = new Set(o.presenti || [])
  let ora = 0, picco = 0
  const coda = creaCoda({
    scarica: async (s) => {
      chiesti.push(s); ora++; picco = Math.max(picco, ora)
      await pausa(5)
      ora--
      if (esito(s)) { presenti.add(s); return true }
      return false
    },
    ce: s => presenti.has(s),
    log: s => logs.push(s),
    ...o,
  })
  const fine = async () => { for (let i = 0; i < 400 && (coda.attivi || coda.inAttesa); i++) await pausa(5) }
  return { coda, chiesti, logs, presenti, picco: () => picco, fine }
}

test('limiti di fabbrica: pochi eventi insieme, coda con tetto', () => {
  assert.equal(INSIEME, 2)
  assert.equal(MASSIMO, 4096)
})

test("un evento con 700 pezzi inventati costa UNA richiesta di file, non 700", async () => {
  const p = prova(() => false)
  assert.equal(p.coda.accoda(Array.from({ length: 700 }, (_, i) => sha(i + 1)), null), true)
  await p.fine()
  assert.equal(p.chiesti.length, 1, `chiesti ${p.chiesti.length}`)
  assert.equal(p.coda.inAttesa, 0)
  assert.ok(p.logs.some(l => l.includes('699 non chiesti')), p.logs.join('\n'))
  assert.ok(!p.logs.some(l => /[0-9a-f]{64}/.test(l)), 'nel log niente hash')
})

test('i file di un evento si chiedono in ordine, saltando quelli già sul nodo e i doppioni', async () => {
  const p = prova(() => true, { presenti: [sha(2)] })
  p.coda.accoda([sha(1), sha(2), sha(3), sha(1)], null)
  await p.fine()
  assert.deepEqual(p.chiesti, [sha(1), sha(3)])
})

test('mai più di INSIEME eventi al lavoro insieme, anche con 50 eventi in arrivo', async () => {
  const p = prova(() => true)
  for (let i = 0; i < 50; i++) p.coda.accoda([sha(1000 + i)], null)
  assert.equal(p.coda.attivi, INSIEME)
  await p.fine()
  assert.equal(p.chiesti.length, 50)
  assert.equal(p.picco(), INSIEME)
})

test('oltre il tetto i file di un evento nuovo non si replicano (e si dice quanti, senza hash)', async () => {
  const p = prova(() => true, { massimo: 10 })
  assert.equal(p.coda.accoda(Array.from({ length: 8 }, (_, i) => sha(i + 1)), null), true)
  assert.equal(p.coda.accoda([sha(100), sha(101), sha(102)], null), false)
  assert.ok(p.logs.some(l => l.includes('coda di replica piena')), p.logs.join('\n'))
  await p.fine()
  assert.equal(p.chiesti.length, 8)
  // svuotata la coda, si riprende
  assert.equal(p.coda.accoda([sha(100)], null), true)
  await p.fine()
  assert.equal(p.chiesti.length, 9)
})

test('uno scaricatore che lancia vale come file non arrivato', async () => {
  const p = prova(() => true, { scarica: async () => { throw new Error('rete') } })
  p.coda.accoda([sha(1), sha(2)], null)
  await p.fine()
  assert.equal(p.coda.inAttesa, 0)
  assert.ok(p.logs.some(l => l.includes('1 non chiesti')), p.logs.join('\n'))
})

test('ferma: niente più lavori', async () => {
  const p = prova(() => true)
  p.coda.ferma()
  assert.equal(p.coda.accoda([sha(1)], null), false)
  await pausa(20)
  assert.equal(p.chiesti.length, 0)
})
