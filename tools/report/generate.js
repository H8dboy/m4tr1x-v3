#!/usr/bin/env node
// @ts-check
'use strict'
/**
 * Rigenera le parti automatiche di docs/STATO_M4TR1X.md (tra i marcatori
 * <!-- AUTO:NOME:INIZIO --> e <!-- AUTO:NOME:FINE -->):
 *   REGOLE      stato di ogni regola I-xx letto dal CODICE dei test (non dal documento),
 *               esito delle prove di sabotaggio, divergenze tra documento e codice
 *   TEST        esecuzione di ogni file di test, con ambiente e data
 *   DIPENDENZE  versioni installate (lockfile) e vulnerabilità (npm audit)
 * Scrive anche docs/generated/report.json.
 *
 * Uso: npm run report   (con la versione di Node fissata)
 */
const fs = require('fs')
const os = require('os')
const path = require('path')
const { spawnSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..', '..')
const DOC = path.join(ROOT, 'docs', 'STATO_M4TR1X.md')
const PINNED = fs.readFileSync(path.join(ROOT, '.node-version'), 'utf8').trim()
const now = new Date()
const today = now.toISOString().slice(0, 10)

// ─── Regole ──────────────────────────────────────────────────────────────────
function docRules() {
  const src = fs.readFileSync(path.join(ROOT, 'docs', 'SECURITY_INVARIANTS.md'), 'utf8')
  /** @type {Map<string, { text: string, test: string, phase: string }>} */
  const m = new Map()
  for (const line of src.split('\n')) {
    if (!/^\| I-\d+ \|/.test(line)) continue
    const cells = line.split('|').slice(1, -1).map(c => c.trim())
    if (cells.length < 4) continue
    const [id, text, test, phase] = cells
    // la frase in grassetto è l'enunciato della regola: si usa intera (niente tagli a metà)
    const bold = /\*\*(.+?)\*\*/.exec(text)
    let plain = (bold ? bold[1] : text).replace(/`/g, '').replace(/\s+/g, ' ').trim()
    // grassetto di una o due parole ("Conservazione:"): si aggiunge il resto della frase
    if (bold && plain.length < 30) plain = (plain + ' ' + text.slice(bold.index + bold[0].length).replace(/\*\*|`/g, '').split(/\. |\.$/)[0]).replace(/\s+/g, ' ').trim()
    const short = plain.length > 140 ? plain.slice(0, 137) + '…' : plain
    m.set(id, { text: short, test: (/`(inv-[\w-]+)`/.exec(test) || [])[1] || '', phase })
  }
  return m
}

function pendingRules() {
  const src = fs.readFileSync(path.join(ROOT, 'tests', 'invariants', 'pending.test.js'), 'utf8')
  /** @type {Map<string, string>} */ const m = new Map()
  for (const r of src.matchAll(/\[\s*['"](I-\d+)['"]\s*,\s*['"].*?['"]\s*,\s*['"](F\w+)['"]\s*\]/g)) m.set(r[1], r[2])
  return m
}

function activeRules() {
  /** @type {Map<string, string>} id → file */ const m = new Map()
  for (const f of fs.readdirSync(path.join(ROOT, 'tests', 'invariants'))) {
    const mm = /^inv-([\d-]+?)-[a-z]/.exec(f)
    if (!mm || !f.endsWith('.test.js')) continue
    for (const n of mm[1].split('-')) m.set(`I-${String(Number(n)).padStart(2, '0')}`, f)
  }
  return m
}

function sabotage() {
  const p = path.join(ROOT, 'docs', 'generated', 'sabotage.json')
  if (!fs.existsSync(p)) return null
  return JSON.parse(fs.readFileSync(p, 'utf8'))
}

// ─── Test ────────────────────────────────────────────────────────────────────
function walk(/** @type {string} */ d) {
  return fs.readdirSync(d, { withFileTypes: true }).flatMap(e => e.isDirectory() ? walk(path.join(d, e.name)) : [path.join(d, e.name)])
}

function runTests() {
  const files = walk(path.join(ROOT, 'tests')).filter(f => f.endsWith('.test.js')).sort()
  return files.map(f => {
    const rel = path.relative(ROOT, f).replace(/\\/g, '/')
    const t0 = Date.now()
    const r = spawnSync(process.execPath, ['--test', '--test-reporter=tap', '--test-force-exit', '--test-timeout=120000', rel],
      { cwd: ROOT, encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, env: process.env })
    const out = (r.stdout || '') + (r.stderr || '')
    const num = (/** @type {string} */ k) => { const x = new RegExp(`^# ${k} (\\d+)`, 'm').exec(out); return x ? Number(x[1]) : 0 }
    const skipReasons = [...out.matchAll(/# SKIP (.*)$/gm)].map(x => x[1].trim())
    return { file: rel, pass: num('pass'), fail: num('fail'), todo: num('todo'), skipped: num('skipped'), skipReasons, ms: Date.now() - t0 }
  })
}

// ─── Dipendenze ──────────────────────────────────────────────────────────────
function deps() {
  const pkg = JSON.parse(fs.readFileSync(path.join(ROOT, 'package.json'), 'utf8'))
  const lock = JSON.parse(fs.readFileSync(path.join(ROOT, 'package-lock.json'), 'utf8'))
  const installed = (/** @type {string} */ n) => lock.packages?.[`node_modules/${n}`]?.version || '?'
  const list = [
    ...Object.entries(pkg.dependencies || {}).map(([n, v]) => ({ name: n, declared: v, installed: installed(n), kind: 'produzione' })),
    ...Object.entries(pkg.devDependencies || {}).map(([n, v]) => ({ name: n, declared: v, installed: installed(n), kind: 'solo sviluppo' })),
  ]
  const transitive = Object.keys(lock.packages || {}).filter(k => k.startsWith('node_modules/')).length - list.length
  let audit = null
  const r = spawnSync(process.platform === 'win32' ? 'npm.cmd' : 'npm', ['audit', '--json'], { cwd: ROOT, encoding: 'utf8', shell: process.platform === 'win32' })
  try { audit = JSON.parse(r.stdout).metadata?.vulnerabilities || null } catch {}
  return { list, transitive, audit }
}

// ─── Scrittura ───────────────────────────────────────────────────────────────
function table(/** @type {string[]} */ head, /** @type {string[][]} */ rows) {
  return [`| ${head.join(' | ')} |`, `|${head.map(() => '---').join('|')}|`, ...rows.map(r => `| ${r.join(' | ')} |`)].join('\n')
}

function main() {
  const env = `Node ${process.versions.node} · ${os.platform()} ${os.release()} · ${today}`
  const pinnedOk = process.versions.node === PINNED
  const doc = docRules(), pend = pendingRules(), act = activeRules(), sab = sabotage()
  const ids = [...new Set([...doc.keys(), ...pend.keys(), ...act.keys()])].sort((a, b) => Number(a.slice(2)) - Number(b.slice(2)))

  const tests = runTests()
  const byFile = new Map(tests.map(t => [path.basename(t.file), t]))
  const divergences = []
  const ruleRows = ids.map(id => {
    const d = doc.get(id), file = act.get(id), ph = pend.get(id)
    const status = file && !ph ? 'attiva' : ph ? 'in attesa' : 'ASSENTE'
    if (!d) divergences.push(`${id}: ha un test ma non è nel documento delle regole`)
    if (file && ph) divergences.push(`${id}: ha un test attivo ma è ancora nell'elenco "in attesa"`)
    if (!file && !ph) divergences.push(`${id}: nel documento ma senza test e senza voce "in attesa"`)
    if (d && file && d.test && !file.startsWith(d.test)) divergences.push(`${id}: il documento indica il test \`${d.test}\`, il codice usa \`${file}\``)
    if (d && ph && d.phase !== ph) divergences.push(`${id}: fase ${d.phase} nel documento, ${ph} nell'elenco "in attesa"`)
    const t = file ? byFile.get(file) : null
    const testCell = file ? `\`${file}\`${t ? (t.fail ? ` ❌ ${t.fail} falliti` : ` ✅ ${t.pass} verdi`) : ''}` : '—'
    const srows = sab ? sab.results.filter((/** @type {any} */ r) => r.rule === id) : []
    const sabCell = !file ? '—' : !sab ? 'mai eseguita' : !srows.length ? '**nessuna prova di sabotaggio**' :
      `${srows.filter((/** @type {any} */ r) => r.outcome === 'presa').length}/${srows.length} prese` +
      (srows.some((/** @type {any} */ r) => r.outcome !== 'presa') ? ' ⚠ ' + srows.filter((/** @type {any} */ r) => r.outcome !== 'presa').map((/** @type {any} */ r) => `${r.name}: ${r.outcome}`).join('; ') : '')
    return [id, d ? d.text : '(non documentata)', status, d?.phase || ph || '?', testCell, sabCell]
  })

  const active = ruleRows.filter(r => r[2] === 'attiva').length
  const regole = [
    `_Generato da \`npm run report\` il ${today}. Fonte: il codice dei test (\`tests/invariants/\`), confrontato con \`docs/SECURITY_INVARIANTS.md\`._`,
    '',
    `**${active} regole attive, ${ruleRows.filter(r => r[2] === 'in attesa').length} in attesa, ${ruleRows.filter(r => r[2] === 'ASSENTE').length} assenti.**` +
      (sab ? ` Prove di sabotaggio: ${sab.results.filter((/** @type {any} */ r) => r.outcome === 'presa').length}/${sab.results.length} prese, eseguite il ${sab.date.slice(0, 10)} (Node ${sab.node}, ${sab.platform}).` : ' Prove di sabotaggio: mai eseguite con `npm run sabotage`.'),
    '',
    table(['Regola', 'Cosa garantisce', 'Stato', 'Fase', 'Test', 'Sabotaggio'], ruleRows),
    '',
    divergences.length ? '**Divergenze tra documento e codice:**\n\n' + divergences.map(x => `- ${x}`).join('\n') : '**Divergenze tra documento e codice:** nessuna trovata.',
  ].join('\n')

  const tot = tests.reduce((a, t) => ({ pass: a.pass + t.pass, fail: a.fail + t.fail, todo: a.todo + t.todo, skipped: a.skipped + t.skipped }), { pass: 0, fail: 0, todo: 0, skipped: 0 })
  const testSec = [
    `_Generato da \`npm run report\`: ogni file di test eseguito da solo. Ambiente: ${env}._`,
    pinnedOk ? '' : `\n> ⚠ **Questi risultati NON valgono:** eseguiti con Node ${process.versions.node}, la versione fissata è ${PINNED}. Usare \`npm run report\`.\n`,
    `**Totale: ${tot.pass} verdi, ${tot.fail} falliti, ${tot.skipped} saltati, ${tot.todo} in attesa.**`,
    '',
    table(['File', 'Verdi', 'Falliti', 'Saltati', 'In attesa', 'Durata'], tests.map(t => [
      `\`${t.file}\``, String(t.pass), t.fail ? `**${t.fail}**` : '0', t.skipped ? `${t.skipped} (${t.skipReasons.join('; ').slice(0, 80)})` : '0', String(t.todo), `${(t.ms / 1000).toFixed(1)} s`])),
  ].join('\n')

  const d = deps()
  const depSec = [
    `_Generato da \`npm run report\` il ${today}, dal lockfile e da \`npm audit\`._`,
    '',
    table(['Pacchetto', 'Uso', 'Dichiarata', 'Installata'], d.list.map(x => [`\`${x.name}\``, x.kind, x.declared, x.installed])),
    '',
    `Dipendenze indirette (installate da quelle sopra): **${d.transitive}**.`,
    d.audit ? `Vulnerabilità (npm audit): critiche **${d.audit.critical}**, alte **${d.audit.high}**, medie ${d.audit.moderate}, basse ${d.audit.low}.` : 'Vulnerabilità: `npm audit` non eseguito (rete assente?) — **non verificato**.',
  ].join('\n')

  // misure a freddo della rete Tor (tools/tor/misura.js): non sono test, servono a vedere se peggiora
  const torFile = path.join(ROOT, 'docs', 'generated', 'tor-tempi.json')
  const misure = fs.existsSync(torFile) ? JSON.parse(fs.readFileSync(torFile, 'utf8')).misure || [] : []
  const torSec = [
    `_Generato da \`npm run report\` dallo storico di \`npm run misura:tor\` (misura a freddo: nodo nuovo al primo avvio). Non è un test: non blocca niente._`,
    '',
    misure.length ? table(['Data', 'Ambiente', 'Riusciti', 'Riserva 5 min', 'tor-a', 'pubblicazione', 'rete', 'evento+file'],
      misure.slice(-10).map((/** @type {any} */ m) => [m.data.slice(0, 16).replace('T', ' '), m.ambiente, `${m.riusciti}/${m.giri}`, `${m.riserve}/${m.giri}`,
        ...['tor-a', 'pubblicazione', 'rete'].map(k => m.mediane[k] == null ? '—' : `${m.mediane[k]} s`),
        m.mediane.evento == null ? '—' : `${(m.mediane.evento + (m.mediane.file || 0)).toFixed(1)} s`])) : 'Nessuna misura ancora: `M4TR1X_TOR_BIN=… npm run misura:tor`.',
  ].join('\n')

  let md = fs.readFileSync(DOC, 'utf8')
  if (!md.includes('<!-- AUTO:TOR:INIZIO -->')) md = md.replace('### 11.3 Prove che mancano', '### 11.2b Misure a freddo della rete Tor (storico)\n\n<!-- AUTO:TOR:INIZIO -->\n<!-- AUTO:TOR:FINE -->\n\n### 11.3 Prove che mancano')
  for (const [name, body] of [['REGOLE', regole], ['TEST', testSec], ['DIPENDENZE', depSec], ['TOR', torSec]]) {
    const re = new RegExp(`(<!-- AUTO:${name}:INIZIO -->)[\\s\\S]*?(<!-- AUTO:${name}:FINE -->)`)
    if (!re.test(md)) throw new Error(`marcatori AUTO:${name} assenti in STATO_M4TR1X.md`)
    md = md.replace(re, `$1\n${body}\n$2`)
  }
  fs.writeFileSync(DOC, md)
  fs.mkdirSync(path.join(ROOT, 'docs', 'generated'), { recursive: true })
  fs.writeFileSync(path.join(ROOT, 'docs', 'generated', 'report.json'), JSON.stringify({ date: now.toISOString(), env, pinnedOk, totals: tot, tests, rules: ruleRows, divergences, deps: d }, null, 2) + '\n')
  console.log(`report aggiornato: ${tot.pass} verdi, ${tot.fail} falliti · ${active} regole attive · ${divergences.length} divergenze`)
  if (!pinnedOk) { console.error(`ATTENZIONE: Node ${process.versions.node} ≠ ${PINNED}`); process.exitCode = 1 }
}

main()
