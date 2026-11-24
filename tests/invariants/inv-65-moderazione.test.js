// @ts-check
'use strict'
/**
 * I-65 — La moderazione automatica guarda solo contenuti PUBBLICI e mette bandierine sui CONTENUTI, mai sulle
 * persone, e non toglie niente da sola (docs/design/MODERAZIONE.md, decisione del fondatore 24/11/2026).
 *  (1) kind fuori da KIND_PUBBLICI (buste, sigilli, annunci DM, …): nessuna bandierina, il testo non si guarda;
 *  (2) l'analizzatore usa solo kind e testo: non nomina chi scrive e non tiene niente tra una chiamata e l'altra;
 *  (3) una bandierina ha solo categoria, peso, id delle regole e «scritto in codice»: nessun testo, nessuna persona;
 *  (4) il linguaggio in codice (numeri al posto di lettere, lettere separate, sosia di altri alfabeti, caratteri
 *      invisibili) non sfugge, ed è segnato come tale;
 *  (5) un elenco di regole storto si rifiuta tutto.
 * Le regole qui sono esempi INNOCUI: l'elenco vero non sta nel repo.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const { analizza, validaRegole, KIND_PUBBLICI } = require('../../packages/moderazione')

const REGOLE = validaRegole({ versione: 1, regole: [
  { id: 'r1', categoria: 'truffa', schema: 'soldi facili', peso: 2 },
  { id: 'r2', categoria: 'spam', schema: 'clicca qui', peso: 1 },
  { id: 'r3', categoria: 'altro', schema: 'parolaproibita', peso: 1 },
] })
const ev = (/** @type {number} */ kind, /** @type {string} */ content) => ({ kind, content, pubkey: 'a'.repeat(64), id: 'b'.repeat(64), sig: 'c'.repeat(128), tags: [], created_at: 1 })

test('I-65 (1): fuori dai kind pubblici niente bandierine, anche con il testo di una regola', () => {
  for (const kind of [1059, 13, 14, 4, 10477, 0, 3, 7, 1984, 24242, 30000])
    assert.deepEqual(analizza(ev(kind, 'soldi facili, clicca qui'), REGOLE), [], `kind ${kind} analizzato`)
  assert.deepEqual([...KIND_PUBBLICI].sort((a, b) => a - b), [1, 1111, 30023], 'elenco dei kind pubblici cambiato senza rivedere I-65')
  // un evento "busta" con un contenuto che, se letto, farebbe scattare la regola: getter che registra la lettura
  let letto = false
  const busta = { kind: 1059, get content() { letto = true; return 'soldi facili' } }
  analizza(busta, REGOLE)
  assert.equal(letto, false, 'il testo di un kind non pubblico è stato letto')
})

test('I-65 (2, 3): bandierine sul contenuto, con soli campi fissi; niente di chi scrive', () => {
  const b = analizza(ev(1, 'Guarda: SOLDI FACILI per tutti, clicca qui!'), REGOLE)
  assert.deepEqual(b.map(x => x.categoria), ['truffa', 'spam'])
  for (const x of b) {
    assert.deepEqual(Object.keys(x).sort(), ['categoria', 'codice', 'peso', 'regole'])
    assert.ok(x.regole.every(id => /^[a-z0-9-]{1,32}$/.test(id)))
  }
  const testo = JSON.stringify(b)
  for (const s of ['a'.repeat(64), 'b'.repeat(64), 'soldi', 'clicca']) assert.ok(!testo.includes(s), `nella bandierina: ${s.slice(0, 10)}`)
  // nessuna memoria tra le chiamate: lo stesso contenuto dà sempre lo stesso esito, qualunque cosa sia venuta prima
  const primo = analizza(ev(1, 'clicca qui'), REGOLE)
  for (let i = 0; i < 50; i++) analizza(ev(1, 'clicca qui, soldi facili'), REGOLE)
  assert.deepEqual(analizza(ev(1, 'clicca qui'), REGOLE), primo, 'l\'esito dipende da quante volte si è già visto qualcosa')
})

test('I-65 (2): il modulo non nomina chi scrive, non importa niente e non tiene stato', () => {
  const src = fs.readFileSync(path.join(__dirname, '..', '..', 'packages', 'moderazione', 'index.js'), 'utf8').replace(/\/\/[^\n]*|\/\*[\s\S]*?\*\//g, '')
  assert.ok(!/pubkey|\.sig\b|autore|author/.test(src), 'l\'analizzatore nomina chi scrive')
  assert.ok(!/require\(/.test(src), 'l\'analizzatore importa moduli (rete, file, …)')
  // a livello di modulo solo costanti congelate o espressioni regolari: nessun Map/Set/array che cresca
  const fuori = src.split('\n').filter(r => /^(let|var) /.test(r) || /^const \w+ = (new (Map|Set)|\[)/.test(r))
  assert.deepEqual(fuori, [], 'stato a livello di modulo')
})

test('I-65 (4): il linguaggio in codice non sfugge ed è segnato come tale', () => {
  const chiaro = analizza(ev(1, 'soldi facili'), REGOLE)[0]
  assert.equal(chiaro.codice, false)
  assert.equal(chiaro.peso, 2)
  for (const [come, testo] of [
    ['numeri al posto di lettere', 's0ld1 f4c1l1'],
    ['lettere separate', 's.o.l.d.i f-a-c-i-l-i'],
    ['sosia cirillici', 'ѕоldі fасіlі'],
    ['caratteri invisibili', 'sol​di fac‍ili'],
    ['lettere ripetute e simboli', 'sooooldi f@ciliii'],
    ['maiuscole e numeri', 'S0LDI FACIL1'],
    ['lettere larghe', 'ＳＯＬＤＩ ＦＡＣＩＬＩ'],
    ['lettere matematiche', '𝐬𝐨𝐥𝐝𝐢 𝐟𝐚𝐜𝐢𝐥𝐢'],
  ]) {
    const b = analizza(ev(1, `ecco ${testo} per te`), REGOLE).find(x => x.categoria === 'truffa')
    assert.ok(b, `${come}: sfuggito`)
    assert.equal(b.codice, true, `${come}: non segnato come scritto in codice`)
    assert.equal(b.peso, 3, `${come}: scritto in codice ma non pesa di più`)
  }
  // "cose strane" senza nessun elenco: invisibili dentro le parole, alfabeti mischiati
  assert.deepEqual(analizza(ev(1, 'una pa​rola qualsiasi'), validaRegole({ versione: 1, regole: [] })).map(b => [b.categoria, b.regole]), [['offuscamento', ['invisibili']]])
  assert.deepEqual(analizza(ev(1, 'ciaо a tutti'), validaRegole({ versione: 1, regole: [] })).map(b => [b.categoria, b.regole]), [['offuscamento', ['alfabeti-mischiati']]])
  // niente falsi allarmi sul testo normale, accenti compresi
  assert.deepEqual(analizza(ev(1, 'Caffè con gli amici, poi una pizza: è stata una bella giornata!'), REGOLE), [])
  // una regola corta vale solo come parola intera, mai dentro un'altra parola
  const corta = validaRegole({ versione: 1, regole: [{ id: 'c1', categoria: 'altro', schema: 'oca', peso: 1 }] })
  assert.deepEqual(analizza(ev(1, 'la vocale e il tocco'), corta), [])
  assert.equal(analizza(ev(1, 'una oca nel parco'), corta).length, 1)
  // …e un carattere invisibile dentro la parola corta non la fa sfuggire (per le corte non c'è il confronto "senza spazi")
  const nascosta = analizza(ev(1, 'una o​ca nel parco'), corta).find(b => b.categoria === 'altro')
  assert.ok(nascosta && nascosta.codice, 'una regola corta con un invisibile dentro sfugge')
})

test('I-65 (5): un elenco di regole storto si rifiuta tutto', () => {
  const buona = { id: 'r1', categoria: 'truffa', schema: 'soldi facili', peso: 2 }
  for (const [come, elenco] of [
    ['categoria inventata', { versione: 1, regole: [{ ...buona, categoria: 'opinioni' }] }],
    ['categoria riservata', { versione: 1, regole: [{ ...buona, categoria: 'offuscamento' }] }],
    ['peso fuori scala', { versione: 1, regole: [{ ...buona, peso: 9 }] }],
    ['id doppio', { versione: 1, regole: [buona, { ...buona }] }],
    ['schema troppo corto', { versione: 1, regole: [{ ...buona, schema: 'a!' }] }],
    ['campo in più (es. chi segnala)', { versione: 1, regole: [{ ...buona, segnalata_da: 'x' }] }],
    ['campo in più nell\'elenco', { versione: 1, regole: [buona], persone: ['x'] }],
    ['versione mancante', { regole: [buona] }],
  ]) assert.throws(() => validaRegole(elenco), /regole non valide/, come)
})
