// @ts-check
'use strict'
/**
 * Runner del sabotaggio (tools/sabotage/esito.js): una prova è "presa" SOLO se il suo test era verde sul
 * codice intatto (fondatore, 21/10/2026) e cade con il sabotaggio per la regola, non per altro.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const { leggi, problemaBase, esitoSabotaggio, fuoriPiattaforma, nonPassa, nuoviResidui, scegli } = require('../../tools/sabotage/esito')

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

test('spazio su disco esaurito (ENOSPC) sul codice intatto = AMBIENTE, non base-rossa (falso rosso)', () => {
  const l = leggi(tap(2, 1) + 'Error: ENOSPC: no space left on device, write\n')
  assert.equal(l.ambiente, true)
  const base = problemaBase(l)
  assert.match(String(base), /^AMBIENTE/)
  assert.equal(esitoSabotaggio(base, leggi(tap(0, 5)), 1000).outcome, 'ambiente')
  // anche il messaggio del kernel "No space left on device" vale come ambiente
  assert.equal(leggi(tap(1, 0) + 'No space left on device\n').ambiente, true)
  // non è un verde: fa comunque notare che bisogna rilanciare (come ogni esito diverso da presa)
  assert.equal(nonPassa('ambiente'), true)
})

test('con la base verde: caduto = presa, tutto verde = sfuggita, non compila / tempo = non vale', () => {
  assert.equal(esitoSabotaggio(null, leggi(tap(2, 1)), 1000).outcome, 'presa')
  assert.equal(esitoSabotaggio(null, leggi('TAP version 13\n'), 1000).outcome, 'presa')
  assert.equal(esitoSabotaggio(null, leggi(tap(3, 0)), 1000).outcome, 'sfuggita')
  assert.equal(esitoSabotaggio(null, leggi(tap(2, 1) + 'could not compile `x`\n'), 1000).outcome, 'non-compila')
  assert.equal(esitoSabotaggio(null, leggi(tap(2, 1) + 'timed out after 1000ms\n'), 1000).outcome, 'interrotta')
  assert.equal(esitoSabotaggio(null, leggi(tap(2, 1), new Error('x')), 1000).outcome, 'interrotta')
})

test('prova di un\'altra piattaforma: non eseguita, mai "presa", non fa fallire la campagna', () => {
  assert.equal(fuoriPiattaforma({}, 'win32'), null)
  assert.equal(fuoriPiattaforma({ piattaforma: 'linux' }, 'linux'), null)
  const e = fuoriPiattaforma({ piattaforma: 'linux' }, 'win32')
  assert.equal(e?.outcome, 'altra-piattaforma')
  assert.equal(nonPassa('presa'), false)
  assert.equal(nonPassa('altra-piattaforma'), false)
  for (const o of ['sfuggita', 'base-rossa', 'non-compila', 'interrotta', 'non-applicabile']) assert.equal(nonPassa(o), true, o)
  const src = require('fs').readFileSync(require('path').join(__dirname, '..', '..', 'tools', 'sabotage', 'run.js'), 'utf8')
  // il testo della prova si controlla PRIMA della piattaforma: una prova Linux che invecchia si vede anche su Windows
  assert.ok(src.indexOf("outcome: 'non-applicabile'") < src.indexOf('fuoriPiattaforma(m, process.platform)'), 'piattaforma controllata prima del testo')
  assert.ok(src.indexOf('fuoriPiattaforma(m, process.platform)') < src.indexOf('const problema = base(m.test)'), 'piattaforma controllata dopo la base')
})

test('scelta delle prove: tutte, per id, solo questa piattaforma; nessuna scelta = errore', () => {
  const M = [{ id: 'i37-a' }, { id: 'i37-b', piattaforma: 'linux' }, { id: 'i58-c', piattaforma: 'win32' }]
  assert.deepEqual(scegli(M, undefined, 'linux'), { scelte: M, parziale: false })
  assert.deepEqual(scegli(M, 'i37', 'linux').scelte.map(m => m.id), ['i37-a', 'i37-b'])
  const lin = scegli(M, '--piattaforma-attuale', 'linux')
  assert.deepEqual(lin.scelte.map(m => m.id), ['i37-b'])
  assert.equal(lin.parziale, true, 'una corsa per piattaforma non deve sovrascrivere la campagna completa')
  assert.deepEqual(scegli(M, '--piattaforma-attuale', 'win32').scelte.map(m => m.id), ['i58-c'])
  assert.throws(() => scegli(M, '--piattaforma-attuale', 'darwin'), /nessuna prova/, 'job senza prove = verde vuoto')
  assert.throws(() => scegli(M, 'inesistente', 'linux'), /nessuna prova/)
  // le prove vere: su Linux ce ne sono, quindi il job CI ha qualcosa da eseguire
  const tutte = require('../../tools/sabotage/mutations')
  assert.ok(scegli(tutte, '--piattaforma-attuale', 'linux').scelte.length >= 2, 'nessuna prova Linux per il job sabotaggio-linux')
})

test('il runner chiede la base PRIMA di applicare il sabotaggio e salta le prove con base rossa', () => {
  const src = require('fs').readFileSync(require('path').join(__dirname, '..', '..', 'tools', 'sabotage', 'run.js'), 'utf8')
  const iBase = src.indexOf('const problema = base(m.test)')
  const iScrivi = src.indexOf('fs.writeFileSync(file, edits.reduce(')
  assert.ok(iBase > 0 && iScrivi > iBase, 'la base non è chiesta prima di applicare il sabotaggio')
  assert.match(src.slice(iBase, iScrivi), /if \(problema\) \{[\s\S]*?continue\n  \}/, 'con la base rossa il sabotaggio viene applicato comunque')
  assert.match(src, /esitoSabotaggio\(null, esegui\(m\.test/, 'il sabotaggio non usa esitoSabotaggio')
})

test('residui: contano solo i file non tracciati comparsi DOPO la prova; il runner li controlla dopo prove e basi', () => {
  assert.deepEqual(nuoviResidui(['a.txt'], ['a.txt', 'app/x/esporta_seme.toml']), ['app/x/esporta_seme.toml'])
  assert.deepEqual(nuoviResidui(['a.txt'], ['a.txt']), [])
  const src = require('fs').readFileSync(require('path').join(__dirname, '..', '..', 'tools', 'sabotage', 'run.js'), 'utf8')
  assert.match(src, /ripulisciResidui\(m\.id\)/, 'dopo la prova non si cercano i residui')
  assert.match(src, /ripulisciResidui\(`base /, 'dopo la base non si cercano i residui')
  assert.match(src, /residuiTotali\.length \? 1 : 0/, 'un residuo non fa fallire la campagna')
})
