// @ts-check
'use strict'
/**
 * I-31 — Nessuna chiave privata utente raggiunge il codice JavaScript dell'app;
 * CSP senza `unsafe-inline`.
 *
 * Parte statica (attiva da F4, guscio Tauri):
 *  (1) CSP stretta in tauri.conf.json, prototipi congelati, devtools spenti, niente URL remoti;
 *  (2) il JavaScript può chiamare SOLO i comandi elencati in build.rs (app manifest), e
 *      i permessi delle capability sono solo quelli: nessun plugin, nessuna API core;
 *  (3) i comandi restituiscono solo tipi di `api.rs` (o tipi semplici), e in `api.rs`
 *      non c'è nessun campo che sembri una chiave o un segreto;
 *  (4) interfaccia senza script inline, gestori on*=, javascript:, eval, innerHTML.
 * Parte dinamica (con l'identità, F4): ogni comando chiamato con un seme di prova
 * non restituisce mai byte del seme o delle chiavi — test aggiunto con i comandi di identità.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const { spawnSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..', '..')
const SHELL = path.join(ROOT, 'app', 'shell')
const UI = path.join(ROOT, 'app', 'ui')
const read = (/** @type {string} */ p) => fs.readFileSync(p, 'utf8')
const walk = (/** @type {string} */ d) => fs.readdirSync(d, { withFileTypes: true })
  .flatMap(e => e.isDirectory() ? walk(path.join(d, e.name)) : [path.join(d, e.name)])

/** Comandi dichiarati in build.rs */
function manifestCommands() {
  const m = /const COMMANDS: &\[&str\] = &\[([^\]]*)\]/.exec(read(path.join(SHELL, 'build.rs')))
  assert.ok(m, 'COMMANDS non trovato in build.rs')
  return [...m[1].matchAll(/"([a-z_]+)"/g)].map(x => x[1])
}

/** Funzioni #[tauri::command] nel sorgente del guscio, con il tipo restituito */
function commandFns() {
  /** @type {{ name: string, ret: string, file: string }[]} */ const out = []
  for (const f of walk(path.join(SHELL, 'src')).filter(f => f.endsWith('.rs'))) {
    const src = read(f)
    for (const m of src.matchAll(/#\[tauri::command[^\]]*\]\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*(?:<R: Runtime>)?\([^)]*\)\s*(?:->\s*([^{]+))?\{/g)) {
      out.push({ name: m[1], ret: (m[2] || '()').trim(), file: path.relative(ROOT, f) })
    }
  }
  return out
}

test('CSP stretta, prototipi congelati, niente devtools né URL remoti', () => {
  const conf = JSON.parse(read(path.join(SHELL, 'tauri.conf.json')))
  const sec = conf.app.security
  const csp = sec.csp
  assert.equal(typeof csp, 'object', 'CSP come oggetto (direttiva per direttiva)')
  const all = Object.values(csp).join(' ')
  for (const bad of ["'unsafe-inline'", "'unsafe-eval'", "'wasm-unsafe-eval'", 'http:', 'https:', 'ws:', 'wss:', 'data:', '*'])
    assert.ok(!all.split(/\s+/).includes(bad), `CSP contiene ${bad}`)
  assert.equal(csp['default-src'], "'self'")
  assert.equal(csp['script-src'], "'self'")
  assert.equal(csp['object-src'], "'none'")
  assert.equal(csp['base-uri'], "'none'")
  assert.deepEqual(csp['connect-src'].split(/\s+/).sort(), ['http://ipc.localhost', 'ipc:'])
  assert.equal(sec.freezePrototype, true)
  assert.ok(!sec.dangerousDisableAssetCspModification, 'modifica della CSP degli asset disattivata')
  assert.ok(!sec.dangerousRemoteDomainIpcAccess, 'nessun dominio remoto con accesso IPC')
  for (const w of conf.app.windows) {
    assert.equal(w.devtools, false, `devtools accesi sulla finestra ${w.label}`)
    assert.ok(!w.url || !/^[a-z]+:/i.test(w.url), `finestra ${w.label} carica un URL remoto`)
  }
})

test('il JavaScript può chiamare solo i comandi elencati, e i permessi sono solo quelli', () => {
  const cmds = manifestCommands()
  const fns = commandFns().map(c => c.name).sort()
  assert.deepEqual([...cmds].sort(), fns, 'build.rs e #[tauri::command] non coincidono')
  const lib = read(path.join(SHELL, 'src', 'lib.rs'))
  const h = /generate_handler!\[([^\]]*)\]/.exec(lib)
  assert.ok(h, 'generate_handler! non trovato')
  assert.deepEqual(h[1].split(',').map(s => s.trim()).filter(Boolean).sort(), fns, 'generate_handler! espone altro')
  const allowed = new Set(cmds.map(c => `allow-${c.replace(/_/g, '-')}`))
  for (const f of walk(path.join(SHELL, 'capabilities')).filter(f => f.endsWith('.json'))) {
    const cap = JSON.parse(read(f))
    assert.ok(!cap.remote, `${path.basename(f)}: capability con accesso remoto`)
    for (const p of cap.permissions) {
      const id = typeof p === 'string' ? p : p.identifier
      assert.ok(allowed.has(id), `${path.basename(f)}: permesso non ammesso "${id}" (plugin o API core)`)
    }
  }
})

test('i comandi restituiscono solo tipi di api.rs (le parole solo ai comandi del seme), senza campi che sembrino chiavi', () => {
  const api = read(path.join(SHELL, 'src', 'api.rs'))
  const apiTypes = [...api.matchAll(/pub\s+(?:struct|enum)\s+(\w+)/g)].map(m => m[1])
  const semeTypes = [...read(path.join(SHELL, 'src', 'api_seme.rs')).matchAll(/pub\s+(?:struct|enum)\s+(\w+)/g)].map(m => m[1])
  const SIMPLE = /^(?:\(\)|bool|u8|u16|u32|u64|i32|i64|String)$/
  for (const c of commandFns()) {
    const inner = c.ret.replace(/^Result<\s*/, '').replace(/\s*,\s*String\s*>$/, '')
      .replace(/^Option<(.*)>$/, '$1').replace(/^Vec<(.*)>$/, '$1').trim()
    // I-58: il tipo con le 24 parole lo restituiscono solo i comandi seme_* (finestra del seme)
    if (semeTypes.includes(inner)) {
      assert.ok(/^seme_/.test(c.name), `${c.file}: il comando ${c.name} restituisce le parole del seme ma non è un comando seme_*`)
      continue
    }
    // unica eccezione: media_file restituisce i byte di un file pubblico, accettati solo dopo hash e tipo (I-17)
    if (inner === 'tauri::ipc::Response') { assert.equal(c.name, 'media_file', `${c.file}: ${c.name} restituisce byte grezzi`); continue }
    assert.ok(SIMPLE.test(inner) || apiTypes.includes(inner), `${c.file}: il comando ${c.name} restituisce ${c.ret}, non un tipo di api.rs`)
  }
  assert.ok(!semeTypes.some(t => apiTypes.includes(t)), 'il tipo delle parole è anche in api.rs')
  const fields = [...api.matchAll(/^\s*pub\s+(\w+)\s*:/gm)].map(m => m[1])
  for (const f of fields)
    assert.ok(!/seed|seme|secret|segret|priv|mnemonic|nsec|key|chiav|token|passphrase|cookie/i.test(f), `campo sospetto in api.rs: ${f}`)
})

test('interfaccia senza script inline, gestori on*=, eval o innerHTML', () => {
  for (const f of walk(UI)) {
    const src = read(f), rel = path.relative(ROOT, f)
    if (f.endsWith('.html')) {
      assert.ok(!/<script(?![^>]*\bsrc=)[^>]*>/i.test(src), `${rel}: script inline`)
      assert.ok(!/\son[a-z]+\s*=/i.test(src), `${rel}: gestore on*= nell'HTML`)
      assert.ok(!/javascript:/i.test(src), `${rel}: URL javascript:`)
      assert.ok(!/\sstyle\s*=/i.test(src), `${rel}: stile inline (bloccato dalla CSP)`)
    }
    if (f.endsWith('.js')) {
      for (const [re, what] of [[/\beval\s*\(/, 'eval'], [/new\s+Function\s*\(/, 'new Function'], [/\.innerHTML\s*=/, 'innerHTML'],
        [/\.outerHTML\s*=/, 'outerHTML'], [/insertAdjacentHTML/, 'insertAdjacentHTML'], [/document\.write/, 'document.write']])
        assert.ok(!(/** @type {RegExp} */ (re)).test(src), `${rel}: ${what}`)
    }
  }
})

// Chiave d'identità per i post (F4, fondatore 20/10/2026): si legge dal portachiavi SOLO al momento della
// firma e non resta in memoria. Nel guscio non esiste mai una ChiaveRuolo; il portachiavi si legge solo
// dentro `firma_come_identita`, chiamata solo dal comando `pubblica`.
test('chiave d\'identità: letta solo per firmare, mai tenuta dal guscio', () => {
  const sorgenti = walk(path.join(SHELL, 'src')).filter(f => f.endsWith('.rs'))
  for (const f of sorgenti) {
    // solo il CODICE: un commento che spiega la regola non deve farla scattare
    const src = read(f).replace(/\/\/[^\n]*/g, '')
    assert.ok(!/\bChiaveRuolo\b/.test(src), `${path.relative(ROOT, f)}: il guscio tiene una ChiaveRuolo`)
    assert.ok(!/portachiavi::carica|\bcarica\(/.test(src), `${path.relative(ROOT, f)}: il guscio legge chiavi dal portachiavi`)
  }
  const lib = read(path.join(SHELL, 'src', 'lib.rs')).replace(/\/\/[^\n]*/g, '')
  // UNA sola lettura della chiave nel guscio: dentro `firma_con_la_mia_identita`, che usano solo i comandi
  // che pubblicano (post e profilo)
  const usi = [...lib.matchAll(/firma_come_identita\(/g)].length
  assert.equal(usi, 1, `firma_come_identita usata ${usi} volte (attesa: una, in firma_con_la_mia_identita)`)
  const aiuto = /fn firma_con_la_mia_identita\([\s\S]*?\n}\n/.exec(lib)
  assert.ok(aiuto && aiuto[0].includes('firma_come_identita('), 'firma_come_identita fuori da firma_con_la_mia_identita')
  const chiamanti = [...lib.matchAll(/async fn (\w+)\([^)]*\)[^{]*\{[\s\S]*?\n}\n/g)].filter(m => m[0].includes('firma_con_la_mia_identita(')).map(m => m[1]).sort()
  assert.deepEqual(chiamanti, ['forum_pubblica', 'forum_rispondi', 'media_pubblica', 'profilo_salva', 'pubblica'], `la tua identità firma da: ${chiamanti}`)
  const pc = read(path.join(ROOT, 'app', 'identity', 'src', 'portachiavi.rs'))
  assert.match(pc, /let v = Zeroizing::new\(v\);/, 'il buffer letto dal portachiavi non si cancella')
  assert.match(pc, /fn leggi\(&self, voce: &str\) -> Result<Option<Zeroizing<\[u8; 32\]>>, IdentityError>;/, 'il portachiavi restituisce il segreto senza Zeroizing')
})

const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })
test('chiave d\'identità (Rust): ogni firma legge il portachiavi, segreti ZeroizeOnDrop', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const app = path.join(ROOT, 'app')
  const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(app, 'Cargo.toml'), '-p', 'm4tr1x-identity', '--test', 'firma_identita'],
    { cwd: app, encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  for (const p of ['ogni_firma_legge_il_portachiavi_nessuna_chiave_tenuta', 'senza_identita_salvata_non_si_firma', 'segreti_cancellati_per_costruzione'])
    assert.match(out, new RegExp(`${p} \\.\\.\\. ok`), `manca o fallisce la prova ${p}`)
})

// Prova DINAMICA (21/10/2026): ogni comando chiamato dalla vera IPC di Tauri (runtime finto, capability
// vere), con un seme creato dall'app; nessuna risposta contiene byte delle chiavi o parole del seme.
// Tor e nodo di prova tolti dall'ambiente: la prova non apre niente e non tocca il portachiavi vero.
test('nessuna chiave nelle risposte dei comandi (Rust, IPC vera con un seme di prova)', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 1800000 }, () => {
  const app = path.join(ROOT, 'app')
  const env = { ...process.env }
  delete env.M4TR1X_TOR_BIN
  delete env.M4TR1X_NODO_PROVA
  const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(app, 'Cargo.toml'), '-p', 'm4tr1x-shell', '--test', 'i31_ipc'],
    { cwd: app, env, encoding: 'utf8', timeout: 1800000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  assert.match(out, /nessuna_chiave_raggiunge_il_javascript \.\.\. ok/, 'manca o fallisce la prova nessuna_chiave_raggiunge_il_javascript')
})
