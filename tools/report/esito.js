// @ts-check
'use strict'
/**
 * Esegue UN file di test e ne decide l'esito per `npm run report`.
 *
 * Regola: un file di test che non risponde conta come FALLITO, sempre.
 * "Verde" vuol dire che i test hanno girato e sono passati: non basta l'assenza
 * di errori. Il 20/10/2026 un file (I-37) si è fermato senza stampare niente e il
 * report lo contava 0 verdi, 0 falliti → totale "tutto verde". Mai più.
 *
 * Il file conta come fallito (almeno 1) se:
 *   - il processo è stato fermato (tempo massimo, segnale, errore di avvio)
 *   - il codice di uscita non è 0
 *   - manca il riepilogo TAP (`# tests N`) o dice 0 test
 *   - qualche test è stato annullato (`# cancelled N`)
 *   - il file non ha registrato nessun test (Node lo riporta come UN test verde col
 *     nome del file: `ok 1 - x.test.js`)
 * In quei casi `problema` dice perché e `uscita` tiene le ultime righe dell'output.
 *
 * Il file gira nello stesso processo del runner (`--experimental-test-isolation=none`):
 * con l'isolamento normale un file che esce prima del tempo (`process.exit(0)`)
 * viene riportato da Node come verde, e i test non ancora eseguiti spariscono.
 * Senza isolamento l'uscita anticipata porta via anche il riepilogo, e questo la prende.
 * (provato con Node 22.23.3 il 20/10/2026)
 */
const fs = require('fs')
const path = require('path')
const { spawnSync } = require('child_process')

const RIGHE_USCITA = 80

/**
 * Tempi massimi: UNA sola fonte per report, `npm test` e CI, così un risultato vale ovunque.
 * I test lenti (cargo, tor vero) dichiarano il loro `timeout` nel test stesso, che vale uguale ovunque.
 * TEST_MS: per i test senza `timeout` proprio. FILE_MS: limite duro per file (le prove con tor
 * vero possono aspettare la riserva di 5 minuti più volte); oltre, il processo viene ucciso.
 */
const TEMPI = Object.freeze({ TEST_MS: 120000, FILE_MS: 3600000 })

/** File di test (*.test.js) sotto le cartelle date, o i file stessi; percorsi relativi a cwd, ordinati. */
function fileDiTest(/** @type {string} */ cwd, /** @type {string[]} */ voci) {
  const walk = (/** @type {string} */ d) => fs.readdirSync(d, { withFileTypes: true }).flatMap(e => e.isDirectory() ? walk(path.join(d, e.name)) : [path.join(d, e.name)])
  return [...new Set(voci.flatMap(v => {
    const p = path.resolve(cwd, v)
    if (!fs.existsSync(p)) throw new Error(`file di test inesistente: ${v}`)
    return fs.statSync(p).isDirectory() ? walk(p).filter(f => f.endsWith('.test.js')) : [p]
  }).map(f => path.relative(cwd, f).replace(/\\/g, '/')))].sort()
}

/**
 * @param {{ status: number | null, signal: string | null, error?: Error, out: string, file?: string }} r
 */
function esito(r) {
  const num = (/** @type {string} */ k) => { const x = new RegExp(`^# ${k} (\\d+)`, 'm').exec(r.out); return x ? Number(x[1]) : null }
  const tests = num('tests'), cancelled = num('cancelled') || 0
  let fail = num('fail') || 0
  const problemi = []
  if (r.error) problemi.push(`processo fermato: ${r.error.message}`)
  else if (r.signal) problemi.push(`processo ucciso dal segnale ${r.signal}`)
  else if (r.status !== 0) problemi.push(`codice di uscita ${r.status}`)
  if (tests === null) problemi.push('nessun riepilogo dei test: il file non ha finito')
  else if (tests === 0) problemi.push('0 test eseguiti')
  if (cancelled) problemi.push(`${cancelled} test annullati`)
  if (r.file && new RegExp(`^ok \\d+ - (?:.*[\\\\/])?${path.basename(r.file).replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`, 'm').test(r.out)) problemi.push('il file non ha registrato nessun test')
  const problema = problemi.length ? problemi.join('; ') : null
  // gli annullati sono falliti; un file con un problema e nessun fallito ne conta uno
  fail += cancelled
  if (problema && !fail) fail = 1
  return {
    pass: num('pass') || 0, fail, todo: num('todo') || 0, skipped: num('skipped') || 0,
    skipReasons: [...r.out.matchAll(/# SKIP (.*)$/gm)].map(x => x[1].trim()),
    // diagnostiche dei test (t.diagnostic: fasi della prova dei due nodi, tempo dell'onion…), anche da verde:
    // sono lo storico con cui capire se un rosso è rete o codice. Fuori il riepilogo finale del runner.
    diagnostiche: [...r.out.matchAll(/^\s*# (?!Subtest:|(?:tests|suites|pass|fail|cancelled|skipped|todo|duration_ms) \d)(.+)$/gm)].map(x => x[1].trim()),
    problema,
    uscita: problema ? r.out.trimEnd().split('\n').slice(-RIGHE_USCITA).join('\n') : undefined,
  }
}

/**
 * @param {string} rel file di test, relativo a cwd
 * @param {{ cwd: string, timeoutMs?: number, testTimeoutMs?: number, env?: NodeJS.ProcessEnv, conOutput?: boolean }} o
 *   i tempi diversi da TEMPI servono solo alle prove di questo modulo
 */
function eseguiFile(rel, o) {
  const t0 = Date.now()
  // lanciato da dentro un altro test, il file erediterebbe NODE_TEST_CONTEXT e non stamperebbe il TAP
  const env = { ...(o.env || process.env) }
  delete env.NODE_TEST_CONTEXT
  const r = spawnSync(process.execPath, ['--test', '--experimental-test-isolation=none', '--test-reporter=tap', '--test-force-exit', `--test-timeout=${o.testTimeoutMs || TEMPI.TEST_MS}`, rel],
    { cwd: o.cwd, encoding: 'utf8', timeout: o.timeoutMs || TEMPI.FILE_MS, killSignal: 'SIGKILL', maxBuffer: 64 * 1024 * 1024, env })
  const out = (r.stdout || '') + (r.stderr || '')
  /** @type {ReturnType<typeof esito> & { file: string, ms: number, output?: string, rilancio?: { fase: string, primoGiroMs: number, primoErrore: string } }} */
  const e = { file: rel, ...esito({ status: r.status, signal: r.signal, error: r.error, out, file: rel }), ms: Date.now() - t0 }
  if (o.conOutput) e.output = out
  return e
}

/**
 * Fasi della rete Tor (fondatore, 20/10/2026): un rosso SOLO in queste fasi si rilancia UNA volta.
 * Tutte le prove con tor vero scrivono la fase nell'errore con le stesse parole: `fase "pubblicazione"`
 * (il descrittore non è confermato sugli HSDir) e `fase "rete"` (l'onion non si raggiunge). Ogni altra
 * fase ("evento", "file", "eco", "relay"…) è il nostro codice: resta rossa subito.
 */
// La fase conta solo all'INIZIO del messaggio d'errore (`error: 'fase "rete"…` o `error: |-` e a capo):
// un errore che CITA la frase ("manca la fase "pubblicazione"") non è un rosso di rete (20/10/2026).
const FASE_DI_RETE = /^\s+error: (?:\|-?\s*\n\s+|'|")fase \\?"(pubblicazione|rete)\\?"/m

/**
 * La fase di rete se TUTTI i test falliti del file sono caduti in una fase di rete; altrimenti null.
 * Mai per un file fermato, uscito prima, senza test o con test annullati: quelli non sono la rete.
 * @param {ReturnType<typeof esito>} e @param {string} out
 */
function faseDiRete(e, out) {
  if (!e.fail || e.problema !== 'codice di uscita 1') return null
  // blocchi dei test falliti di primo livello (TAP): "not ok N - nome" fino alla fine del blocco YAML
  const falliti = out.split(/^not ok \d+ - /m).slice(1).map(b => b.split(/^ {2}\.\.\.$/m)[0])
  if (falliti.length !== e.fail) return null
  const fasi = falliti.map(b => FASE_DI_RETE.exec(b)?.[1])
  return fasi.every(Boolean) ? /** @type {string} */ (fasi[0]) : null
}

/**
 * Come `eseguiFile`, ma un file rosso SOLO per la rete Tor si rilancia una volta (mai di più), e il rilancio
 * resta scritto nel risultato (`rilancio`) con l'errore del primo giro.
 * @param {string} rel @param {Parameters<typeof eseguiFile>[1]} o
 */
function eseguiConRilancio(rel, o) {
  const primo = eseguiFile(rel, { ...o, conOutput: true })
  const fase = faseDiRete(primo, /** @type {string} */ (primo.output))
  delete primo.output
  if (!fase) return primo
  const secondo = eseguiFile(rel, o)
  return {
    ...secondo,
    ms: primo.ms + secondo.ms,
    rilancio: { fase, primoGiroMs: primo.ms, primoErrore: (primo.uscita || '').split('\n').filter(l => /fase "/.test(l)).slice(0, 3).join(' | ') },
  }
}

module.exports = { TEMPI, esito, eseguiFile, eseguiConRilancio, faseDiRete, fileDiTest }
