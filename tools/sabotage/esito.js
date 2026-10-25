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
    // spazio su disco esaurito durante il test: non è un rosso reale, è l'ambiente (24/10/2026)
    ambiente: /No space left on device/i.test(out),
  }
}

/** @typedef {ReturnType<typeof leggi>} Lettura */

/**
 * Il test sul codice INTATTO: null se è verde, altrimenti perché non lo è.
 * Verde = finito, compilato, almeno un test passato, nessun fallito.
 * @param {Lettura} l
 */
function problemaBase(l) {
  if (l.ambiente) return 'AMBIENTE: spazio su disco esaurito (ENOSPC) sul codice intatto — non è un rosso reale (libera /tmp o imposta TMPDIR su un disco capiente)'
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
 * @returns {{ outcome: 'presa'|'sfuggita'|'interrotta'|'non-compila'|'base-rossa'|'ambiente', detail: string }}
 */
function esitoSabotaggio(base, l, tempo) {
  if (base && base.startsWith('AMBIENTE')) return { outcome: 'ambiente', detail: base }
  if (base) return { outcome: 'base-rossa', detail: `${base}: la prova non dimostra niente, prima va reso verde il test` }
  if (l.errore) return { outcome: 'interrotta', detail: `test interrotto (${l.errore}): alzare timeoutMs della prova` }
  if (l.nonCompila) return { outcome: 'non-compila', detail: 'il codice sabotato non compila: la prova non dimostra niente, va riscritta' }
  if (l.tempoScaduto) return { outcome: 'interrotta', detail: `tempo massimo (${tempo} ms) superato: alzare timeoutMs della prova` }
  if (l.fail === null) return { outcome: 'presa', detail: 'il test è caduto' }
  if (l.fail === 0) return { outcome: 'sfuggita', detail: 'tutti i test verdi con la regola rotta' }
  return { outcome: 'presa', detail: `fail ${l.fail}` }
}

/**
 * Prove che richiedono una CAPACITÀ dell'ambiente (es. il sandbox di rete di I-01: iptables/ipset/ACL con
 * sudo, presente solo nella CI): senza, il test si salta e il sabotaggio "sfuggirebbe" senza essere un buco.
 * Esito `non-applicabile-qui`: visibile nel report, MAI contato come presa, non fa fallire la campagna.
 * @param {{ richiede?: string }} m @param {Record<string, boolean>} capacita cosa c'è su questa macchina
 * @returns {{ outcome: 'non-applicabile-qui', detail: string } | null}
 */
function nonApplicabileQui(m, capacita) {
  if (!m.richiede || capacita[m.richiede]) return null
  return { outcome: 'non-applicabile-qui', detail: `serve la capacità "${m.richiede}", assente qui: la prova gira nella CI` }
}

/**
 * Prove di difese che esistono su UNA piattaforma (21/10/2026: 0700 e ambiente pulito su Linux, ACL su
 * Windows): altrove il codice sabotato non è nemmeno compilato e risulterebbe "sfuggita" senza motivo.
 * Esito `altra-piattaforma`: visibile nel report, MAI contato come presa, non fa fallire la campagna.
 * @param {{ piattaforma?: string }} m @param {string} qui process.platform
 * @returns {{ outcome: 'altra-piattaforma', detail: string } | null}
 */
function fuoriPiattaforma(m, qui) {
  if (!m.piattaforma || m.piattaforma === qui) return null
  return { outcome: 'altra-piattaforma', detail: `prova solo per ${m.piattaforma}: non eseguita su ${qui}, va eseguita lì` }
}

/**
 * Quali prove eseguire. Senza argomenti: tutte (campagna completa, salvata). `<testo>`: quelle il cui id lo
 * contiene (parziale). `--piattaforma-attuale` (21/10/2026, job CI per le difese di una sola piattaforma):
 * SOLO quelle segnate per questa piattaforma (parziale). Nessuna prova scelta = errore, mai un verde vuoto.
 * @template {{ id: string, piattaforma?: string }} M
 * @param {M[]} tutte @param {string|undefined} arg @param {string} qui
 * @returns {{ scelte: M[], parziale: boolean }}
 */
function scegli(tutte, arg, qui) {
  const scelte = arg === '--piattaforma-attuale' ? tutte.filter(m => m.piattaforma === qui) : tutte.filter(m => m.id.includes(arg || ''))
  if (!scelte.length) throw new Error(`nessuna prova di sabotaggio scelta (${arg || 'tutte'} su ${qui}): niente da dimostrare`)
  return { scelte, parziale: !!arg }
}

/** Esiti che fanno fallire la campagna: tutto tranne `presa` e `altra-piattaforma`. */
const nonPassa = (/** @type {string} */ outcome) => outcome !== 'presa' && outcome !== 'altra-piattaforma' && outcome !== 'non-applicabile-qui'

/** File non tracciati comparsi dopo una prova (residui di un build sabotato): dopo meno prima. */
const nuoviResidui = (/** @type {string[]} */ prima, /** @type {string[]} */ dopo) => { const p = new Set(prima); return dopo.filter(f => !p.has(f)) }

module.exports = { leggi, problemaBase, esitoSabotaggio, fuoriPiattaforma, nonApplicabileQui, nonPassa, nuoviResidui, scegli }
