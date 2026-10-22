#!/usr/bin/env node
// @ts-check
'use strict'
/**
 * Unico modo di unire una PR su main (decisione del fondatore, 22/10/2026). GitHub gratuito non ha check
 * obbligatori sui repo privati: questo script fa da cancello finché il repo non diventa pubblico al lancio.
 * MAI merge dal sito.
 *
 * Unisce in squash SOLO se, sull'ultimo commit della PR:
 *  - ogni job della CI (quelli di ci.yml + l'elenco fisso, i01-rete per primo) ha un esito di GitHub Actions;
 *  - l'ultimo tentativo di ognuno è `success` (non saltato, non neutro, non in corso);
 *  - la PR è aperta, non in bozza, verso main; il merge è legato a quel commit (--match-head-commit).
 * Uso: node tools/merge.js <numero PR> [--verifica]   (--verifica: decide e non unisce)
 */
const fs = require('fs')
const path = require('path')
const { spawnSync } = require('child_process')

const REPO = 'H8dboy/m4tr1x-v3'
// restano obbligatori anche se qualcuno li togliesse da ci.yml
const FISSI = ['i01-rete', 'test', 'secrets', 'deps', 'tor', 'sabotaggio-linux']
// solo sui tag v*: su una PR risulta saltato per costruzione
const SOLO_TAG = ['release-gate']

/** Nomi dei job in ci.yml (chiavi con due spazi sotto `jobs:`). @param {string} yml */
function jobsDiCi(yml) {
  const dopo = yml.slice(yml.indexOf('\njobs:\n'))
  return [...dopo.matchAll(/^ {2}([A-Za-z0-9_-]+):\s*$/gm)].map(m => m[1])
}

/** @param {string} yml */
function obbligatori(yml) {
  return [...new Set([...FISSI, ...jobsDiCi(yml)])].filter(n => !SOLO_TAG.includes(n))
}

/**
 * @param {{ state: string, isDraft: boolean, baseRefName: string }} pr
 * @param {Array<{ name: string, status: string, conclusion: string|null, started_at: string, app?: { slug?: string } }>} checks
 * @param {string[]} richiesti
 * @returns {{ ok: boolean, motivi: string[] }}
 */
function decidi(pr, checks, richiesti) {
  const motivi = []
  if (pr.state !== 'OPEN') motivi.push(`PR non aperta (${pr.state})`)
  if (pr.isDraft) motivi.push('PR in bozza')
  if (pr.baseRefName !== 'main') motivi.push(`PR verso ${pr.baseRefName}, non main`)
  if (!richiesti.includes('i01-rete')) motivi.push('i01-rete non è tra i job obbligatori')
  for (const nome of richiesti) {
    // solo GitHub Actions: un'altra app potrebbe pubblicare un esito con lo stesso nome
    const suoi = checks.filter(c => c.name === nome && c.app?.slug === 'github-actions')
      .sort((a, b) => String(b.started_at).localeCompare(String(a.started_at)))
    const ultimo = suoi[0]
    if (!ultimo) motivi.push(`${nome}: nessun esito sull'ultimo commit`)
    else if (ultimo.status !== 'completed') motivi.push(`${nome}: ancora ${ultimo.status}`)
    else if (ultimo.conclusion !== 'success') motivi.push(`${nome}: ${ultimo.conclusion}`)
  }
  return { ok: motivi.length === 0, motivi }
}

module.exports = { jobsDiCi, obbligatori, decidi, FISSI }

/** @param {string[]} args */
function gh(...args) {
  const r = spawnSync('gh', args, { encoding: 'utf8', shell: false })
  if (r.status !== 0) throw new Error(`gh ${args[0]}: ${r.stderr || r.stdout}`)
  return r.stdout
}

if (require.main === module) {
  const numero = process.argv[2]
  if (!/^\d+$/.test(numero || '')) { console.error('uso: node tools/merge.js <numero PR> [--verifica]'); process.exit(2) }
  const pr = JSON.parse(gh('pr', 'view', numero, '--repo', REPO, '--json', 'state,isDraft,baseRefName,headRefOid'))
  const sha = String(pr.headRefOid)
  const checks = JSON.parse(gh('api', `repos/${REPO}/commits/${sha}/check-runs?per_page=100`)).check_runs
  const yml = fs.readFileSync(path.join(__dirname, '..', '.github', 'workflows', 'ci.yml'), 'utf8')
  const richiesti = obbligatori(yml)
  const d = decidi(pr, checks, richiesti)
  console.log(`PR #${numero} su ${sha.slice(0, 7)} · obbligatori: ${richiesti.join(', ')}`)
  if (!d.ok) { console.error(`MERGE RIFIUTATO:\n- ${d.motivi.join('\n- ')}`); process.exit(1) }
  if (process.argv.includes('--verifica')) { console.log('tutto verde: si può unire (--verifica, non unito)'); process.exit(0) }
  gh('pr', 'merge', numero, '--repo', REPO, '--squash', '--match-head-commit', sha)
  console.log(`unita in squash (commit verificato ${sha.slice(0, 7)})`)
}
