// @ts-check
'use strict'
/**
 * Esito di una prova di sabotaggio, separato da run.js perché si possa provare (tests/unit/sabotage-esito.test.js).
 *
 * Regola (suggerimento del fondatore, 21/10/2026): prima di sabotare, il test deve essere VERDE sul codice
 * intatto. Se è già rosso, "è caduto con il sabotaggio" non dimostra niente: l'esito è `base-rossa`,
 * mai `presa`. Successo il 21/10/2026 con I-28 (5 "prese" con un test già rosso).
 */

/**
 * Cosa ha detto un'esecuzione di `node --test` (uscita TAP).
 * @param {string} out @param {Error|undefined} [error]
 */
function leggi(out, error) {
  const num = (/** @type {string} */ k) => {
    const righe = out.split('\n').filter(l => l.startsWith(`# ${k} `))
    return righe.length ? Number(righe.at(-1)?.slice(k.length + 3)) : null
  }
  return {
    errore: error ? error.message : null,
    // Un test fermato dal tempo massimo non ha detto niente.
    tempoScaduto: /testTimeoutFailure|timed out after/.test(out),
    // Codice che non compila: il test cade per l'errore, non per la regola (20/10/2026).
    nonCompila: /error\[E\d{4}\]|could not compile/.test(out),
    fail: num('fail'),
    pass: num('pass'),
  }
}

/** @typedef {ReturnType<typeof leggi>} Lettura */

/**
 * Il test sul codice INTATTO: null se è verde, altrimenti perché non lo è.
 * Verde = finito, compilato, almeno un test passato, nessun fallito.
 * @param {Lettura} l
 */
function problemaBase(l) {
  if (l.errore) return `il test non finisce sul codice intatto (${l.errore})`
  if (l.tempoScaduto) return 'il test supera il tempo massimo sul codice intatto'
  if (l.nonCompila) return 'il codice intatto non compila'
  if (l.fail === null) return 'il test non ha finito sul codice intatto (nessun riepilogo)'
  if (l.fail > 0) return `il test è già rosso sul codice intatto (${l.fail} falliti)`
  if (!l.pass) return 'nessun test eseguito sul codice intatto'
  return null
}

/**
 * Esito del sabotaggio, dato il test sul codice intatto e quello sul codice sabotato.
 * @param {string|null} base problemaBase(...) del codice intatto
 * @param {Lettura} l lettura con il sabotaggio
 * @param {number} tempo tempo massimo usato (per il messaggio)
 * @returns {{ outcome: 'presa'|'sfuggita'|'interrotta'|'non-compila'|'base-rossa', detail: string }}
 */
function esitoSabotaggio(base, l, tempo) {
  if (base) return { outcome: 'base-rossa', detail: `${base}: la prova non dimostra niente, prima va reso verde il test` }
  if (l.errore) return { outcome: 'interrotta', detail: `test interrotto (${l.errore}): alzare timeoutMs della prova` }
  if (l.nonCompila) return { outcome: 'non-compila', detail: 'il codice sabotato non compila: la prova non dimostra niente, va riscritta' }
  if (l.tempoScaduto) return { outcome: 'interrotta', detail: `tempo massimo (${tempo} ms) superato: alzare timeoutMs della prova` }
  if (l.fail === null) return { outcome: 'presa', detail: 'il test è caduto' }
  if (l.fail === 0) return { outcome: 'sfuggita', detail: 'tutti i test verdi con la regola rotta' }
  return { outcome: 'presa', detail: `fail ${l.fail}` }
}

module.exports = { leggi, problemaBase, esitoSabotaggio }
