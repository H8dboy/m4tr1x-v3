// @ts-check
'use strict'
/**
 * I-37 — Il Tor del desktop serve solo onion e non presta i circuiti dell'app
 * (corretta il 16/10/2026, ADR-0016; tutto il desktop dal 21/10/2026). Le prove sono in Rust, nel crate del trasporto
 * (app/transport/tests/inv_37_tor_process.rs); qui si lanciano con cargo, così la
 * regola compare nel report e nelle prove di sabotaggio come le altre.
 *
 * Senza M4TR1X_TOR_BIN girano solo le prove senza tor (hash sbagliato → non parte);
 * con M4TR1X_TOR_BIN (job "tor" della CI) anche: niente host in chiaro senza
 * credenziali, cookie obbligatorio sul controllo, niente fuori da 127.0.0.1,
 * tor muore con l'app, e un servizio onion di prova raggiunto davvero.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const path = require('path')
const fs = require('fs')
const { spawnSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..', '..')
const MANIFEST = path.join(ROOT, 'app', 'Cargo.toml')
const TOR = process.env.M4TR1X_TOR_BIN
const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })
const noCargo = cargo.status !== 0 ? 'cargo non disponibile: prove Rust di I-37 saltate' : false

/** cargo test sul crate del trasporto; `target` = argomenti di selezione, `filter` = nome della prova. */
function cargoTest(/** @type {string[]} */ target, filter = '') {
  const args = ['test', '--manifest-path', MANIFEST, '-p', 'm4tr1x-transport', ...target, '--', '--nocapture', '--test-threads=4']
  if (filter) args.push(filter)
  const r = spawnSync('cargo', args,
    { cwd: path.join(ROOT, 'app'), encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, env: process.env, shell: process.platform === 'win32' })
  return { code: r.status, out: (r.stdout || '') + (r.stderr || '') }
}

test('I-37 senza tor: binario con hash sbagliato non parte; porta di controllo solo da 127.0.0.1', { skip: noCargo, timeout: 900000 }, () => {
  const unit = cargoTest(['--lib'], 'tor_process::')
  assert.equal(unit.code, 0, unit.out.slice(-4000))
  assert.match(unit.out, /file_della_porta_solo_loopback \.\.\. ok/)
  // 20/10/2026: tentativo con tempo massimo (prima restava appeso ~120 s) e cause leggibili dei rifiuti
  assert.match(unit.out, /tentativo_appeso_chiuso_al_tempo_massimo \.\.\. ok/)
  assert.match(unit.out, /codici_estesi_di_tor_leggibili \.\.\. ok/)
  const src = fs.readFileSync(path.join(ROOT, 'app', 'transport', 'src', 'tor_process.rs'), 'utf8')
  const socksPort = /"--SocksPort",\s*"([^"]+)"/.exec(src)
  assert.ok(socksPort, 'SocksPort non trovata')
  for (const f of ['127.0.0.1:auto', 'OnionTrafficOnly', 'IsolateSOCKSAuth', 'ExtendedErrors'])
    assert.ok(socksPort[1].split(/\s+/).includes(f), `SocksPort senza ${f}: "${socksPort[1]}"`)
  const hash = cargoTest(['--test', 'inv_37_tor_process'], 'binario_con_hash_sbagliato_non_parte')
  assert.equal(hash.code, 0, hash.out.slice(-4000))
  assert.match(hash.out, /binario_con_hash_sbagliato_non_parte \.\.\. ok/)
})

test('I-37 con tor vero: solo onion, cookie, solo 127.0.0.1, muore con l\'app, onion raggiunto', {
  skip: noCargo || (!TOR || !fs.existsSync(TOR) ? 'M4TR1X_TOR_BIN non impostato: prove con tor vero saltate' : false),
  timeout: 900000,
}, (t) => {
  const r = cargoTest(['--test', 'inv_37_tor_process'])
  // anche da verde: il tempo per raggiungere l'onion di prova serve a vedere se peggiora (CI del 20/10:
  // "SOCKS ha risposto 6" dopo 240 s, e nessuno storico con cui confrontarlo)
  // due fasi: "pubblicazione" (rete Tor, rilanciabile) e "connessione" (il nostro trasporto, da indagare)
  t.diagnostic(`onion: ${(/pubblicazione dell'onion di prova: [^\n]*/.exec(r.out) || ['pubblicazione non stampata'])[0]} · ` +
    `${(/onion raggiunto in \d+ s|onion non raggiunto[^\n]*/.exec(r.out) || ['tempo non stampato'])[0]}`)
  t.diagnostic((/tentativi di connessione: [^\n]*/.exec(r.out) || ['tentativi non stampati'])[0])
  // HSDir cercati dal client confrontati con quelli su cui il servizio ha caricato (misura del 20/10/2026)
  t.diagnostic((/hsdir del client: [^\n]*/.exec(r.out) || ['hsdir del client non stampati'])[0])
  // Qui girano PIÙ prove Rust: il runner rilancia una volta i rossi di rete (fase "rete"/"pubblicazione").
  // Se fallisce anche un'altra prova (codice), il rosso NON è solo rete: le parole di fase si neutralizzano.
  const fallite = [...r.out.matchAll(/^test (\w+) \.\.\. FAILED$/gm)].map(m => m[1])
  let messaggio = r.out.slice(-4000)
  if (fallite.some(n => n !== 'connect_onion_raggiunge_un_servizio_onion'))
    messaggio = `NON solo rete: fallite anche ${fallite.join(', ')}\n` + messaggio.replace(/fase "/g, 'fase-non-rilanciabile "')
  else {
    // la fase della prova Rust va IN TESTA al messaggio: il runner la legge solo lì
    const fase = /fase "(\w+)": [^\n]*/.exec(r.out)
    if (fase) messaggio = `${fase[0]}\n${messaggio}`
  }
  assert.equal(r.code, 0, messaggio)
  assert.ok(!r.out.includes('SALTATO: serve M4TR1X_TOR_BIN'), 'le prove Rust si sono saltate anche se M4TR1X_TOR_BIN è impostato')
  // 4 prove + `aiuto_fa_l_app` (che senza la sua variabile non fa niente e risulta passata)
  assert.match(r.out, /test result: ok\. 5 passed/, 'attese le 4 prove di I-37 (più l\'aiuto) passate')
})

// Tor dell'APP (F4, passo 3; tutto il desktop dal 21/10/2026): l'app accetta solo il tor (e su Linux le
// librerie) con gli hash del lock della piattaforma, incluso alla compilazione (app/transport/tests/config_app.rs).
// Con M4TR1X_TOR_DEL_LOCK (tor estratto dal bundle del lock) lo avvia davvero dalla sua copia verificata;
// se la variabile c'è, la prova NON può risultare saltata.
const TOR_DEL_LOCK = process.env.M4TR1X_TOR_DEL_LOCK
test('I-37 app: hash solo dal lock incluso alla compilazione; tor diverso non parte; avvio dalla copia', { skip: noCargo, timeout: 900000 }, (t) => {
  const r = cargoTest(['--test', 'config_app'])
  assert.equal(r.code, 0, r.out.slice(-4000))
  for (const p of ['gli_hash_inclusi_sono_quelli_del_lock', 'senza_tor_indicato_nessuna_configurazione', 'l_app_usa_solo_gli_hash_del_lock', 'tor_con_hash_diverso_non_parte', 'con_il_tor_del_lock_l_app_lo_avvia_dalla_sua_copia'])
    assert.match(r.out, new RegExp(`${p} \\.\\.\\. ok`), `manca o fallisce la prova ${p}`)
  if (TOR_DEL_LOCK) assert.ok(!r.out.includes('SALTATO: serve M4TR1X_TOR_DEL_LOCK'), 'M4TR1X_TOR_DEL_LOCK è impostata ma l\'avvio con il tor del lock si è saltato')
  t.diagnostic(TOR_DEL_LOCK ? 'avvio con il tor del lock: provato' : 'avvio con il tor del lock: non provato (manca M4TR1X_TOR_DEL_LOCK)')
})

// Copia verificata (21/10/2026): lettura unica e verifica in memoria, cartella solo-proprietario (Linux 0700,
// Windows ACL protetta con il solo utente), pulizia senza seguire link o junction, avvio dalla copia con
// l'ambiente pulito su Linux (app/transport/tests/copia_tor.rs). Nessun tor vero.
test('I-37 copia verificata: hash in memoria, solo-proprietario, niente link seguiti, ambiente pulito', { skip: noCargo, timeout: 900000 }, () => {
  const r = cargoTest(['--test', 'copia_tor'])
  assert.equal(r.code, 0, r.out.slice(-4000))
  const prove = ['copia_verificata_e_indipendente_dalla_sorgente', 'hash_sbagliato_o_libreria_mancante_niente_scritto', 'la_pulizia_non_segue_link_o_junction', 'copia_solo_proprietario']
  if (process.platform === 'linux') prove.push('su_linux_tor_parte_con_l_ambiente_pulito')
  for (const p of prove) assert.match(r.out, new RegExp(`${p} \\.\\.\\. ok`), `manca o fallisce la prova ${p}`)
  const src = fs.readFileSync(path.join(ROOT, 'app', 'transport', 'src', 'tor_process.rs'), 'utf8')
  // tor si avvia SOLO dalla copia: nessun Command::new sul percorso indicato
  assert.match(src, /let tor = prepara_binari\(&self\.cfg\)\?;[\s\S]*?Command::new\(&tor\)/, 'tor non parte dalla copia verificata')
  assert.ok(!/Command::new\(&self\.cfg\.tor_bin\)/.test(src), 'tor avviato dal percorso indicato invece che dalla copia')
  // lettura UNICA (contro le sostituzioni tra verifica e copia): una sola lettura dal disco in prepara_binari,
  // e si scrivono i byte verificati, non una seconda lettura della sorgente
  const prep = /pub fn prepara_binari\([\s\S]*?\n}\n/.exec(src)
  assert.ok(prep, 'prepara_binari non trovata')
  assert.equal([...prep[0].matchAll(/std::fs::read(?:_to_string)?\(|File::open\(|\.read_to_end\(|std::fs::copy\(/g)].length, 1, 'prepara_binari legge la sorgente più di una volta')
  assert.match(prep[0], /\.write_all\(&byte\)/, 'la copia non scrive i byte verificati')
  // "crea nuovo": se tra la creazione della cartella e la scrittura compare un file o un collegamento, si
  // fallisce invece di seguirlo (intervallo che una prova a runtime non riesce a colpire: controllo statico)
  assert.match(prep[0], /\.write\(true\)\.create_new\(true\);/, 'i file della copia non sono aperti con create_new')
  assert.ok(!/\.create\(true\)|\.truncate\(true\)/.test(prep[0]), 'prepara_binari apre file esistenti')
})
