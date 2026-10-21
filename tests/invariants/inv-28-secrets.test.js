// @ts-check
'use strict'
/**
 * I-28 — Nessun segreto nel repo.
 * (1) lo scanner riconosce ogni tipo di segreto vietato (esempi finti qui sotto);
 * (2) i file tracciati oggi non ne contengono.
 * In CI gira anche gitleaks sulla storia intera.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const path = require('path')
const { scanFile, scanRepo, oscura } = require('../../tools/security/scan-secrets')

const FAKE = {
  nsec: 'nsec1' + 'q'.repeat(58),
  pem: '-----BEGIN PRIVATE KEY-----',
  onion: 'a'.repeat(56) + '.onion',
  lan: 'http://192.168.1.7:8080',
  pub: 'ssh utente@8.8.4.4',
  mail: 'qualcuno@protonmail.com',
  hex64: 'ab'.repeat(64),
}

for (const [name, sample] of Object.entries(FAKE)) {
  test(`riconosce: ${name}`, () => {
    assert.ok(scanFile('src/x.js', `const v = "${sample}"`).length > 0)
  })
}

for (const f of ['.env', 'config/.env.local', 'data/relay.db', 'h8ledger.db-wal', 'tor/hs_ed25519_secret_key',
  'release.keystore', 'node_config.json', 'h8identity.enc', 'app-release.apk']) {
  test(`file vietato: ${f}`, () => assert.ok(scanFile(f, '').length > 0))
}

test('ammessi: email noreply, i tre blocchi di documentazione (RFC 5737), localhost', () => {
  const ok = 'Co-Authored-By: x <234557612+H8dboy@users.noreply.github.com> 127.0.0.1 192.0.2.10 198.51.100.4 203.0.113.5 version 3.0.0'
  assert.deepEqual(scanFile('docs/x.md', ok), [])
})

// Eccezione stretta (21/10/2026): version="6.0.0.0" nei manifest .xml di Windows. Solo quella forma.
test('ammesso: numero di versione come valore di version="…" in un file .xml', () => {
  const manifest = `<assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" />\n<a version='1.2.3.4'/>`
  assert.deepEqual(scanFile('app/x/manifest.xml', manifest), [])
})

test('eccezione version="…" solo in .xml, solo per il valore intero, solo per version', () => {
  const ip = FAKE.pub.split('@')[1]
  const casi = [
    ['app/x/manifest.xml', `<a version="1.0.0.0" host="${ip}"/>`],            // un IP vero nello stesso file
    ['app/x/manifest.xml', `<a server="${ip}"/>`],                             // altro attributo
    ['app/x/manifest.xml', `<a release="${ip}"/>`],                            // altro attributo lungo quanto version (prova solo il nome)
    ['app/x/manifest.xml', `<a version="${ip}:443"/>`],                        // non è il valore intero
    ['app/x/manifest.xml', `<a version=${ip}/>`],                              // senza virgolette
    ['app/x/manifest.xml', `<a version="${ip}'/>`],                            // virgolette diverse
    ['app/x/manifest.xml', `<a subversion="${ip}"/>`],                         // nome che finisce per version
    ['app/x/manifest.xml', `version: ${ip}`],                                  // non è un attributo
    ['src/x.js', `const a = { version="${ip}" }`],                             // file non .xml
    ['app/x/manifest.xml.js', `<a version="${ip}"/>`],                         // estensione solo simile
  ]
  for (const [f, testo] of casi) assert.ok(scanFile(f, testo).length > 0, `non segnalato: ${f} → ${testo}`)
  // LIMITE dichiarato: dentro version="…" di un .xml qualsiasi numero a quattro parti passa, anche se
  // ha la forma di un IP vero (versione e IP non si distinguono). Se cambia, questa riga lo segnala.
  assert.deepEqual(scanFile('app/x/manifest.xml', `<a version="${ip}"/>`), [])
})

test('un IP vero accanto a uno di documentazione viene comunque segnalato', () => {
  assert.ok(scanFile('src/x.js', 'a = "203.0.113.5"; b = "' + FAKE.pub.split('@')[1] + '"').length > 0)
})

// oscura (21/10/2026): il report copia l'uscita dei test falliti; senza oscurarla un rosso dello scanner
// finiva in report.json e il repo restava rosso anche dopo la correzione.
test('oscura: dopo, lo scanner non trova più niente nel contenuto (ogni tipo di segreto)', () => {
  for (const [name, sample] of Object.entries(FAKE)) {
    const testo = `uscita del test:\n  errore: "${sample}" e ancora ${sample}\n`
    const o = oscura('docs/generated/report.json', testo)
    assert.deepEqual(scanFile('docs/generated/report.json', o), [], `${name}: resta visibile dopo oscura`)
    assert.match(o, /\[oscurato: [^\]]+\]/, `${name}: niente segnaposto`)
  }
})

test('oscura: un testo pulito resta identico (ogni file tracciato del repo, eccezioni comprese)', () => {
  const root = path.resolve(__dirname, '..', '..')
  const { execFileSync } = require('child_process')
  const fs = require('fs')
  const files = execFileSync('git', ['ls-files', '-z'], { cwd: root }).toString('utf8').split('\0').filter(Boolean)
  let controllati = 0
  for (const f of files) {
    const abs = path.join(root, f)
    if (!fs.existsSync(abs) || scanFile(f, '').length) continue
    const buf = fs.readFileSync(abs)
    if (buf.includes(0)) continue
    const testo = buf.toString('utf8')
    if (scanFile(f, testo).length) continue   // file esclusi dal controllo (lo scanner stesso, questo test)
    assert.equal(oscura(f, testo), testo, `${f}: oscura cambia un file pulito`)
    controllati++
  }
  assert.ok(controllati > 100, `controllati solo ${controllati} file`)
  assert.equal(oscura('docs/x.md', 'v 192.0.2.10 198.51.100.4 203.0.113.5 127.0.0.1'), 'v 192.0.2.10 198.51.100.4 203.0.113.5 127.0.0.1')
  assert.equal(oscura('a/m.xml', '<a version="1.0.0.0"/>'), '<a version="1.0.0.0"/>')
})

test('il report scrive STATO_M4TR1X.md e report.json passando da oscura', () => {
  const src = require('fs').readFileSync(path.resolve(__dirname, '..', '..', 'tools', 'report', 'generate.js'), 'utf8')
  assert.match(src, /fs\.writeFileSync\(DOC, oscura\('docs\/STATO_M4TR1X\.md', md\)\)/, 'STATO_M4TR1X.md scritto senza oscura')
  assert.match(src, /fs\.writeFileSync\(path\.join\(ROOT, 'docs', 'generated', 'report\.json'\), oscura\('docs\/generated\/report\.json', JSON\.stringify\(/, 'report.json scritto senza oscura')
  assert.equal([...src.matchAll(/fs\.writeFileSync\(/g)].length, 2, 'il report scrive altri file: passano da oscura?')
})

test('il repo non contiene segreti nei file tracciati', () => {
  const found = scanRepo(path.resolve(__dirname, '..', '..'))
  assert.deepEqual(found, [], found.join('\n'))
})
