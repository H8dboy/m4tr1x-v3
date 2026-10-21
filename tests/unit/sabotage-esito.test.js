// @ts-check
'use strict'
/**
 * Runner del sabotaggio (tools/sabotage/esito.js): una prova è "presa" SOLO se il suo test era verde sul
 * codice intatto (fondatore, 21/10/2026) e cade con il sabotaggio per la regola, non per altro.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const { leggi, problemaBase, esitoSabotaggio } = require('../../tools/sabotage/esito')

const tap = (/** @type {number} */ pass, /** @type {number} */ fail) => `TAP version 13\nok 1 - a\n# tests ${pass + fail}\n# pass ${pass}\n# fail ${fail}\n`

test('base verde: finito, almeno un test passato, nessun fallito', () => {
  assert.equal(problemaBase(leggi(tap(3, 0))), null)
})

test('base NON verde: rossa, senza riepilogo, senza test, non compila, tempo scaduto, interrotta', () => {
  assert.match(String(problemaBase(leggi(tap(2, 1)))), /già rosso/)
  assert.match(String(problemaBase(leggi('TAP version 13\nok 1 - a\n'))), /nessun riepilogo/)
  assert.match(String(problemaBase(leggi(tap(0, 0)))), /nessun test eseguito/)
  assert.match(String(problemaBase(leggi(tap(1, 0) + 'error[E0063]: missing field\n'))), /non compila/)
  assert.match(String(problemaBase(leggi(tap(1, 0) + "failureType: 'testTimeoutFailure'\n"))), /tempo massimo/)
  assert.match(String(problemaBase(leggi(tap(1, 0), new Error('spawnSync ETIMEDOUT')))), /non finisce/)
})

test('con la base rossa il sabotaggio non è MAI "presa", anche se il test cade', () => {
  const base = problemaBase(leggi(tap(2, 1)))
  for (const l of [leggi(tap(0, 5)), leggi('TAP version 13\n'), leggi(tap(3, 0))]) {
    assert.equal(esitoSabotaggio(base, l, 1000).outcome, 'base-rossa')
  }
})

test('con la base verde: caduto = presa, tutto verde = sfuggita, non compila / tempo = non vale', () => {
  assert.equal(esitoSabotaggio(null, leggi(tap(2, 1)), 1000).outcome, 'presa')
  assert.equal(esitoSabotaggio(null, leggi('TAP version 13\n'), 1000).outcome, 'presa')
  assert.equal(esitoSabotaggio(null, leggi(tap(3, 0)), 1000).outcome, 'sfuggita')
  assert.equal(esitoSabotaggio(null, leggi(tap(2, 1) + 'could not compile `x`\n'), 1000).outcome, 'non-compila')
  assert.equal(esitoSabotaggio(null, leggi(tap(2, 1) + 'timed out after 1000ms\n'), 1000).outcome, 'interrotta')
  assert.equal(esitoSabotaggio(null, leggi(tap(2, 1), new Error('x')), 1000).outcome, 'interrotta')
})

test('il runner chiede la base PRIMA di applicare il sabotaggio e salta le prove con base rossa', () => {
  const src = require('fs').readFileSync(require('path').join(__dirname, '..', '..', 'tools', 'sabotage', 'run.js'), 'utf8')
  const iBase = src.indexOf('const problema = base(m.test)')
  const iScrivi = src.indexOf('fs.writeFileSync(file, edits.reduce(')
  assert.ok(iBase > 0 && iScrivi > iBase, 'la base non è chiesta prima di applicare il sabotaggio')
  assert.match(src.slice(iBase, iScrivi), /if \(problema\) \{[\s\S]*?continue\n  \}/, 'con la base rossa il sabotaggio viene applicato comunque')
  assert.match(src, /esitoSabotaggio\(null, esegui\(m\.test/, 'il sabotaggio non usa esitoSabotaggio')
})
