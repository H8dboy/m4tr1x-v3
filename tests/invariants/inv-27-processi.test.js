// @ts-check
'use strict'
/**
 * I-27 — Processi esterni con un tempo massimo, file "bomba" fermati prima di allocare.
 * M4TR1X non usa programmi esterni per i file (niente ffmpeg, exiftool, ImageMagick: pulizia e codifica nel codice
 * Rust, I-16/I-64, con tetti di misura). L'UNICO processo esterno è tor:
 *  (1) statico: solo node/src/tor.js e app/transport/src/tor_process.rs avviano processi; nessun nome di programma
 *      per i file nel codice che gira;
 *  (2) nodo: un tor che non finisce l'avvio viene fermato al tempo massimo e il processo UCCISO (prova dal vivo con
 *      un tor finto, Linux/macOS);
 *  (3) app: tor con tempo massimo per la porta di controllo (30 s, poi ucciso), per il bootstrap, ucciso quando l'app
 *      lo lascia (kill_on_drop) e legato al processo dell'app (__OwningControllerProcess);
 *  (4) un file a celle che dichiara misure enormi si rifiuta PRIMA di allocare i pixel (prova Rust).
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const os = require('os')
const path = require('path')
const { spawnSync } = require('child_process')

const ROOT = path.join(__dirname, '..', '..')
const AMMESSI = ['node/src/tor.js', 'app/transport/src/tor_process.rs']

/** @param {string} dir @returns {string[]} */
function file(dir) {
  const out = []
  for (const e of fs.readdirSync(path.join(ROOT, dir), { withFileTypes: true })) {
    const r = `${dir}/${e.name}`
    if (e.isDirectory()) { if (!['node_modules', 'target', 'tests'].includes(e.name)) out.push(...file(r)) }
    else if (/\.(js|rs)$/.test(e.name)) out.push(r)
  }
  return out
}
const SORGENTI = [
  ...file('node/src'), ...file('packages'),
  ...fs.readdirSync(path.join(ROOT, 'app'), { withFileTypes: true }).filter(e => e.isDirectory() && fs.existsSync(path.join(ROOT, 'app', e.name, 'src'))).flatMap(e => file(`app/${e.name}/src`)),
]
const senzaCommenti = (/** @type {string} */ s) => s.replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:])\/\/[^\n]*/g, '$1')

test('I-27 (1): solo tor si avvia come processo, e nessun programma esterno per i file', () => {
  assert.ok(SORGENTI.length > 30, `troppo pochi file letti (${SORGENTI.length})`)
  const avviano = SORGENTI.filter(f => /child_process|\bexecFile\b|\bspawnSync\b|std::process::Command|tokio::process|Command::new/.test(senzaCommenti(fs.readFileSync(path.join(ROOT, f), 'utf8'))))
  assert.deepEqual(avviano.sort(), [...AMMESSI].sort(), 'un file nuovo avvia processi: serve tempo massimo e una riga in I-27')
  const programmi = SORGENTI.filter(f => /\b(ffmpeg|ffprobe|exiftool|imagemagick|magick|gm\s+convert)\b/i.test(senzaCommenti(fs.readFileSync(path.join(ROOT, f), 'utf8'))))
  assert.deepEqual(programmi, [], 'il codice chiama un programma esterno per i file')
})

test('I-27 (3): il tor dell\'app ha tempi massimi, muore con l\'app e viene ucciso se non parte', () => {
  const rs = fs.readFileSync(path.join(ROOT, 'app/transport/src/tor_process.rs'), 'utf8')
  assert.match(rs, /\.kill_on_drop\(true\)/, 'tor non muore quando l\'app lo lascia')
  assert.match(rs, /"--__OwningControllerProcess", &std::process::id\(\)\.to_string\(\)/, 'tor non è legato al processo dell\'app')
  assert.match(rs, /if t0\.elapsed\(\) > Duration::from_secs\(30\) \{\s*let _ = child\.kill\(\)\.await;/, 'attesa della porta di controllo senza tempo massimo o senza uccidere tor')
  assert.match(rs, /if t0\.elapsed\(\) > self\.cfg\.bootstrap_timeout \{/, 'bootstrap senza tempo massimo')
  const js = fs.readFileSync(path.join(ROOT, 'node/src/tor.js'), 'utf8')
  assert.equal((js.match(/\bspawn\(/g) || []).length, 1, 'il nodo avvia più di un processo')
})

test('I-27 (2): nodo — un tor che non finisce l\'avvio viene fermato al tempo massimo e ucciso', { skip: process.platform === 'win32' ? 'tor finto come script: Linux/macOS (la CI)' : false, timeout: 60000 }, async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'm4i27-'))
  const pidFile = path.join(dir, 'pid')
  /** @type {number|null} */ let pid = null
  try {
    const finto = path.join(dir, 'tor-finto.sh')
    // scrive il suo pid e poi non fa niente: niente "Bootstrapped 100%"
    fs.writeFileSync(finto, `#!/bin/sh\necho $$ > '${pidFile}'\nexec sleep 600\n`, { mode: 0o755 })
    // il nodo gira in un processo a parte con un SUO tempo massimo: un avvio che aspetta per sempre fa fallire
    // la prova invece di bloccarla
    const codice = `require(${JSON.stringify(path.join(ROOT, 'node', 'src', 'tor.js'))}).startTor({ bin: ${JSON.stringify(finto)}, dataDir: ${JSON.stringify(dir)}, ingressPort: 9, log: () => {}, timeoutMs: 1500 })
      .then(() => { console.log('PARTITO'); process.exit(2) }, e => { console.log('NO ' + e.message); process.exit(0) })`
    const t0 = Date.now()
    const r = spawnSync(process.execPath, ['-e', codice], { encoding: 'utf8', timeout: 20000, killSignal: 'SIGKILL' })
    const durata = Date.now() - t0
    if (fs.existsSync(pidFile)) pid = Number(fs.readFileSync(pidFile, 'utf8'))
    assert.equal(r.signal, null, `l'avvio di tor non si è fermato da solo entro 20 s (${durata} ms)`)
    assert.match(r.stdout, /NO Tor: avvio non completato in tempo/, r.stdout + r.stderr)
    assert.ok(durata >= 1400, `fermato troppo presto: ${durata} ms`)
    assert.ok(pid, 'il tor finto non è partito')
    let vivo = true
    for (let i = 0; i < 50 && vivo; i++) {
      try { process.kill(/** @type {number} */ (pid), 0); await new Promise(res => setTimeout(res, 100)) } catch { vivo = false }
    }
    assert.equal(vivo, false, 'il tor che non partiva è ancora vivo')
  } finally {
    if (pid) try { process.kill(pid, 'SIGKILL') } catch {}
    fs.rmSync(dir, { recursive: true, force: true })
  }
})

const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8' })
test('I-27 (4): un file a celle con misure enormi si rifiuta prima di allocare', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 1800000 }, () => {
  const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(ROOT, 'app', 'Cargo.toml'), '-p', 'm4tr1x-celle', '--lib', 'byte_rotti_non_fanno_cadere_niente'], { encoding: 'utf8', maxBuffer: 64 << 20 })
  assert.equal(r.status, 0, `${r.stdout.slice(-3000)}${r.stderr.slice(-3000)}`)
  assert.match(r.stdout, /test [\w:]*byte_rotti_non_fanno_cadere_niente \.\.\. ok/, 'la prova non ha girato')
})
