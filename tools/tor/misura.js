// @ts-check
'use strict'
/**
 * Misura a freddo della rete Tor (NON è un test: non fa fallire nulla; decisione del
 * fondatore, 16/10/2026). Lancia N volte la prova dei due nodi con la cache spenta, come un
 * nodo nuovo al primo avvio, e registra fasi, decisione di annuncio (conferma o riserva) ed
 * esito. Locale: aggiunge allo storico docs/generated/tor-tempi.json (letto da npm run report).
 * In CI: scrive la tabella nel riepilogo del job ($GITHUB_STEP_SUMMARY).
 *
 *   M4TR1X_TOR_BIN=… node tools/tor/misura.js [giri]
 */
const fs = require('fs')
const os = require('os')
const path = require('path')
const { spawnSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..', '..')
const OUT = path.join(ROOT, 'docs', 'generated', 'tor-tempi.json')
const giri = Math.max(1, Number(process.argv[2] || 3))
const ci = !!process.env.GITHUB_ACTIONS

/** @type {{ ok: boolean, annuncio: string, fasi: Record<string, number>, errore?: string }[]} */
const risultati = []
for (let i = 1; i <= giri; i++) {
  const r = spawnSync(process.execPath, ['--test', '--test-force-exit', '--test-reporter=tap', 'tests/integration/two-nodes.test.js'], {
    cwd: ROOT, encoding: 'utf8', timeout: 1500000, maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, M4TR1X_TOR_CACHE: '0' },
  })
  const out = (r.stdout || '') + (r.stderr || '')
  const riga = /# fasi \(cache \S+, annuncio di A: (\S+)\): (.*)$/m.exec(out)
  /** @type {Record<string, number>} */ const fasi = {}
  for (const [, k, v] of (riga?.[2] || '').matchAll(/(\S+)=([\d.]+)s/g)) fasi[k] = Number(v)
  const ok = /^ok 2 /m.test(out)
  const errore = ok ? undefined : (/error: ['"`](.{0,200})/.exec(out)?.[1] || 'esito sconosciuto')
  risultati.push({ ok, annuncio: riga?.[1] || '?', fasi, errore })
  console.log(`giro ${i}: ${ok ? 'ok' : 'FALLITO'} · annuncio ${riga?.[1] || '?'} · ${Object.entries(fasi).map(([k, v]) => `${k}=${v}s`).join(' ')}${errore ? ` · ${errore}` : ''}`)
}

const mediana = (/** @type {number[]} */ xs) => { const s = xs.filter(x => Number.isFinite(x)).sort((a, b) => a - b); return s.length ? s[Math.floor(s.length / 2)] : null }
const voce = {
  data: new Date().toISOString(), ambiente: ci ? 'ci' : `${os.platform()} ${os.release()}`, giri,
  riusciti: risultati.filter(r => r.ok).length,
  riserve: risultati.filter(r => r.annuncio === 'scadenza').length,
  mediane: Object.fromEntries(['tor-a', 'pubblicazione', 'tor-b', 'rete', 'evento', 'file'].map(k => [k, mediana(risultati.map(r => r.fasi[k]))])),
  risultati,
}
const tabella = [
  `### Misura a freddo della rete Tor (${voce.ambiente}, non bloccante)`, '',
  `${voce.riusciti}/${giri} giri riusciti · riserva dei 5 minuti: ${voce.riserve}/${giri}`, '',
  '| fase | mediana (s) |', '|---|---|',
  ...Object.entries(voce.mediane).map(([k, v]) => `| ${k} | ${v ?? '—'} |`),
].join('\n')
console.log('\n' + tabella)
if (ci && process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, tabella + '\n')
if (!ci) {
  const storico = fs.existsSync(OUT) ? JSON.parse(fs.readFileSync(OUT, 'utf8')) : { misure: [] }
  storico.misure.push(voce)
  fs.mkdirSync(path.dirname(OUT), { recursive: true })
  fs.writeFileSync(OUT, JSON.stringify(storico, null, 2) + '\n')
  console.log(`aggiunto allo storico: ${path.relative(ROOT, OUT)} (${storico.misure.length} misure)`)
}
// misura, non test: esce sempre con 0
