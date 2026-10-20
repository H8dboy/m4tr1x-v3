#!/usr/bin/env node
// @ts-check
'use strict'
/**
 * Esegue le prove di sabotaggio (tools/sabotage/mutations.js) e salva l'esito in
 * docs/generated/sabotage.json, letto da `npm run report`.
 *
 * Per ogni modifica: copia di sicurezza del file → modifica → test della regola
 * (con --test-force-exit e timeout) → ripristino SEMPRE, anche se il processo
 * viene interrotto (gestori di uscita) → verifica che il file sia tornato identico.
 *
 * Uso: npm run sabotage            (usa la versione di Node fissata)
 *      npm run sabotage -- i26     (solo le prove il cui id contiene "i26")
 */
const fs = require('fs')
const os = require('os')
const path = require('path')
const crypto = require('crypto')
const { spawnSync } = require('child_process')
const MUTATIONS = require('./mutations')

const ROOT = path.resolve(__dirname, '..', '..')
const OUT = path.join(ROOT, 'docs', 'generated', 'sabotage.json')
const filter = process.argv[2] || ''
const sha = (/** @type {string} */ s) => crypto.createHash('sha256').update(s).digest('hex')

/** @type {Map<string, string>} percorso → contenuto originale */
const pending = new Map()
function restoreAll() {
  for (const [p, orig] of pending) { try { fs.writeFileSync(p, orig) } catch {} }
  pending.clear()
}
for (const sig of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.on(sig, () => { restoreAll(); process.exit(130) })
process.on('exit', restoreAll)
process.on('uncaughtException', e => { restoreAll(); console.error(e); process.exit(1) })

const results = []
for (const m of MUTATIONS.filter(x => x.id.includes(filter))) {
  const file = path.join(ROOT, m.file)
  const orig = fs.readFileSync(file, 'utf8')
  const edits = m.edits || [{ find: /** @type {string} */ (m.find), replace: /** @type {string} */ (m.replace) }]
  const bad = edits.map(e => orig.split(e.find).length - 1).find(c => c !== 1)
  if (bad !== undefined) {
    results.push({ ...pick(m), outcome: 'non-applicabile', detail: `testo da sostituire trovato ${bad} volte: aggiornare la prova` })
    console.log(`NON APPLICABILE  ${m.rule} ${m.name}`)
    continue
  }
  pending.set(file, orig)
  fs.writeFileSync(file, edits.reduce((s, e) => s.replace(e.find, e.replace), orig))
  let outcome, detail
  try {
    // Le prove con tor vero (I-37) durano minuti: ognuna può dichiarare il suo tempo massimo.
    const testTimeout = m.timeoutMs || 90000
    const r = spawnSync(process.execPath, ['--test', '--test-reporter=tap', '--test-force-exit', `--test-timeout=${testTimeout}`, m.test],
      { cwd: ROOT, encoding: 'utf8', timeout: Math.max(400000, testTimeout * 2), maxBuffer: 64 * 1024 * 1024 })
    const out = (r.stdout || '') + (r.stderr || '')
    const fails = out.split('\n').filter(l => l.startsWith('# fail '))
    const last = fails.at(-1)
    // Un test fermato dal tempo massimo NON ha preso il sabotaggio: non si sa cosa avrebbe detto.
    const timedOut = /testTimeoutFailure|timed out after/.test(out)
    // Un sabotaggio che NON COMPILA non è "preso": il test cade per l'errore di compilazione, non perché
    // si accorge della regola rotta. Va riscritto perché compili (20/10/2026: feed-senza-verifica lo era).
    const nonCompila = /error\[E\d{4}\]|could not compile/.test(out)
    if (r.error) { outcome = 'interrotta'; detail = `test interrotto (${r.error.message}): alzare timeoutMs della prova` }
    else if (nonCompila) { outcome = 'non-compila'; detail = 'il codice sabotato non compila: la prova non dimostra niente, va riscritta' }
    else if (timedOut) { outcome = 'interrotta'; detail = `tempo massimo (${testTimeout} ms) superato: alzare timeoutMs della prova` }
    else if (!last) { outcome = 'presa'; detail = 'il test è caduto' }
    else if (last.trim() === '# fail 0') { outcome = 'sfuggita'; detail = 'tutti i test verdi con la regola rotta' }
    else { outcome = 'presa'; detail = last.replace('# ', '') }
  } finally {
    fs.writeFileSync(file, orig)
    pending.delete(file)
  }
  if (sha(fs.readFileSync(file, 'utf8')) !== sha(orig)) throw new Error(`ripristino fallito: ${m.file}`)
  results.push({ ...pick(m), outcome, detail })
  console.log(`${outcome === 'presa' ? 'PRESA     ' : outcome === 'interrotta' ? 'INTERROTTA' : outcome === 'non-compila' ? 'NON COMPILA' : 'SFUGGITA  '}  ${m.rule} ${m.name} — ${detail}`)
}

/** @param {any} m */
function pick(m) { return { id: m.id, rule: m.rule, name: m.name, file: m.file, test: m.test } }

const data = {
  date: new Date().toISOString(),
  node: process.versions.node,
  platform: `${os.platform()} ${os.release()}`,
  partial: !!filter,
  results,
}
fs.mkdirSync(path.dirname(OUT), { recursive: true })
if (!filter) fs.writeFileSync(OUT, JSON.stringify(data, null, 2) + '\n')
const bad = results.filter(r => r.outcome !== 'presa')
console.log(`\n${results.length - bad.length}/${results.length} prese${filter ? ' (parziale: non salvato)' : ` — salvato in ${path.relative(ROOT, OUT)}`}`)
process.exit(bad.length ? 1 : 0)
