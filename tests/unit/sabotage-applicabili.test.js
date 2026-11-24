// @ts-check
'use strict'
/**
 * Ogni prova di sabotaggio si deve poter APPLICARE (tools/sabotage/mutations.js): il suo file esiste, ogni `find`
 * compare una volta sola (le `edits` in ordine, ognuna sul file già modificato dalle precedenti), la modifica cambia
 * davvero qualcosa e il test della regola esiste. Gira a ogni PR in pochi millisecondi.
 * Nato il 24/11/2026: tre sabotaggi (CSP con `blob:`, nomi/titoli del forum, tipi a celle) non trovavano più il loro
 * testo da settimane e nessuno se n'era accorto, perché la campagna completa gira di rado.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const MUTATIONS = require('../../tools/sabotage/mutations')

const ROOT = path.resolve(__dirname, '..', '..')

test('ogni prova di sabotaggio ha id unico, regola, nome e test che esiste', () => {
  const visti = new Set()
  for (const m of MUTATIONS) {
    assert.ok(m.id && !visti.has(m.id), `id mancante o doppio: ${m.id}`)
    visti.add(m.id)
    assert.ok(m.rule && m.name, `${m.id}: regola o nome mancanti`)
    assert.ok(fs.existsSync(path.join(ROOT, m.test)), `${m.id}: il test ${m.test} non esiste`)
  }
})

test('ogni prova di sabotaggio si applica: file presente, ogni find una volta sola, modifica vera', () => {
  /** @type {string[]} */ const rotte = []
  for (const m of MUTATIONS) {
    const file = path.join(ROOT, m.file)
    if (!fs.existsSync(file)) { rotte.push(`${m.id}: file ${m.file} assente`); continue }
    let testo = fs.readFileSync(file, 'utf8')
    const edits = m.edits || [{ find: /** @type {string} */ (m.find), replace: /** @type {string} */ (m.replace) }]
    for (const [i, e] of edits.entries()) {
      const dove = edits.length > 1 ? ` (modifica ${i + 1})` : ''
      if (typeof e.find !== 'string' || typeof e.replace !== 'string' || !e.find) { rotte.push(`${m.id}${dove}: find/replace mancanti`); break }
      if (e.find === e.replace) { rotte.push(`${m.id}${dove}: la modifica non cambia niente`); break }
      const n = testo.split(e.find).length - 1
      if (n !== 1) { rotte.push(`${m.id}${dove}: il testo da sabotare compare ${n} volte in ${m.file}`); break }
      testo = testo.replace(e.find, () => e.replace)
    }
  }
  assert.deepEqual(rotte, [], `prove di sabotaggio che non si applicano più (vanno aggiornate, non ignorate):\n${rotte.join('\n')}`)
})
