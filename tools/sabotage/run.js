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
 *      npm run sabotage -- --piattaforma-attuale   (solo le prove delle difese di QUESTA piattaforma: job CI
 *                                                   "sabotaggio-linux"; nessuna prova scelta = errore)
 */
const fs = require('fs')
const os = require('os')
const path = require('path')
const crypto = require('crypto')
const { spawnSync } = require('child_process')
const MUTATIONS = require('./mutations')
const { leggi, problemaBase, esitoSabotaggio, fuoriPiattaforma, nonPassa, scegli } = require('./esito')

const ROOT = path.resolve(__dirname, '..', '..')
const OUT = path.join(ROOT, 'docs', 'generated', 'sabotage.json')
const { scelte, parziale } = scegli(MUTATIONS, process.argv[2], process.platform)
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

/** @param {string} test @param {number} testTimeout */
function esegui(test, testTimeout) {
  const r = spawnSync(process.execPath, ['--test', '--test-reporter=tap', '--test-force-exit', `--test-timeout=${testTimeout}`, test],
    { cwd: ROOT, encoding: 'utf8', timeout: Math.max(400000, testTimeout * 2), maxBuffer: 64 * 1024 * 1024 })
  return leggi((r.stdout || '') + (r.stderr || ''), r.error)
}

// Prima di sabotare, il test deve essere verde sul codice INTATTO (fondatore, 21/10/2026): una volta per file
// di test, con il tempo massimo più alto tra le prove che lo usano. Le prove con tor vero durano minuti.
/** @type {Map<string, string|null>} test → problemaBase (null = verde) */
const basi = new Map()
const tempoDi = (/** @type {any} */ m) => m.timeoutMs || 90000
function base(/** @type {string} */ test) {
  if (!basi.has(test)) {
    if (pending.size) throw new Error('test sul codice intatto con un sabotaggio ancora applicato')
    const tempo = Math.max(...scelte.filter(x => x.test === test).map(tempoDi))
    const p = problemaBase(esegui(test, tempo))
    basi.set(test, p)
    console.log(`${p ? 'BASE ROSSA ' : 'base verde '}  ${test}${p ? ` — ${p}` : ''}`)
  }
  return /** @type {string|null} */ (basi.get(test))
}

const results = []
for (const m of scelte) {
  const file = path.join(ROOT, m.file)
  const orig = fs.readFileSync(file, 'utf8')
  const edits = m.edits || [{ find: /** @type {string} */ (m.find), replace: /** @type {string} */ (m.replace) }]
  const bad = edits.map(e => orig.split(e.find).length - 1).find(c => c !== 1)
  if (bad !== undefined) {
    results.push({ ...pick(m), outcome: 'non-applicabile', detail: `testo da sostituire trovato ${bad} volte: aggiornare la prova` })
    console.log(`NON APPLICABILE  ${m.rule} ${m.name}`)
    continue
  }
  // dopo il controllo del testo (una prova di un'altra piattaforma che invecchia si vede anche qui)
  const altrove = fuoriPiattaforma(m, process.platform)
  if (altrove) {
    results.push({ ...pick(m), ...altrove })
    console.log(`ALTRA PIATTAFORMA  ${m.rule} ${m.name} — ${altrove.detail}`)
    continue
  }
  const problema = base(m.test)
  if (problema) {
    const e = esitoSabotaggio(problema, leggi(''), tempoDi(m))
    results.push({ ...pick(m), ...e })
    console.log(`BASE ROSSA   ${m.rule} ${m.name} — ${e.detail}`)
    continue
  }
  pending.set(file, orig)
  fs.writeFileSync(file, edits.reduce((s, e) => s.replace(e.find, e.replace), orig))
  let outcome, detail
  try {
    // Le prove con tor vero (I-37) durano minuti: ognuna può dichiarare il suo tempo massimo.
    ;({ outcome, detail } = esitoSabotaggio(null, esegui(m.test, tempoDi(m)), tempoDi(m)))
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
  partial: parziale,
  results,
}
fs.mkdirSync(path.dirname(OUT), { recursive: true })
if (!parziale) fs.writeFileSync(OUT, JSON.stringify(data, null, 2) + '\n')
const bad = results.filter(r => nonPassa(r.outcome))
const altrove = results.filter(r => r.outcome === 'altra-piattaforma').length
const prese = results.filter(r => r.outcome === 'presa').length
console.log(`\n${prese}/${results.length - altrove} prese${altrove ? ` (+${altrove} di un'altra piattaforma, non eseguite qui)` : ''}${parziale ? ' (parziale: non salvato)' : ` — salvato in ${path.relative(ROOT, OUT)}`}`)
process.exit(bad.length ? 1 : 0)
