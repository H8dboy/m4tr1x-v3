// @ts-check
'use strict'
/**
 * I-01 — parte DINAMICA: gli strumenti della prova, provati da soli (la prova vera è il job CI `i01-rete`).
 *  - tools/i01/verifica.js decide il verde: 0 violazioni E traffico vero verso relay e loopback;
 *  - tools/i01/rete.sh: il controllo positivo deve cadere se le regole lasciano uscire qualcosa. Gira solo su
 *    Linux con sudo senza password (runner di GitHub): crea l'utente m4app e le regole, poi le toglie.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const os = require('os')
const path = require('path')
const { spawnSync } = require('child_process')
const { verifica } = require('../../tools/i01/verifica.js')

const ROOT = path.resolve(__dirname, '..', '..')
const RETE = path.join(ROOT, 'tools', 'i01', 'rete.sh')
const riga = (/** @type {number[]} */ [v4, v6, relay, lo]) =>
  `violazioni_ipv4=${v4} violazioni_ipv6=${v6} pacchetti_verso_relay=${relay} pacchetti_loopback=${lo}`

test('verifica: verde solo con 0 violazioni e traffico verso relay e loopback', () => {
  assert.equal(verifica(riga([0, 0, 6274, 2581])).ok, true)
  for (const [valori, motivo] of /** @type {Array<[number[], RegExp]>} */ ([
    [[1, 0, 10, 10], /IPv4 fuori da Tor/],
    [[0, 1, 10, 10], /IPv6 fuori da Tor/],
    [[0, 0, 0, 10], /nessun pacchetto verso i relay/],
    [[0, 0, 10, 0], /nessun pacchetto sul loopback/],
  ])) {
    const r = verifica(riga(valori))
    assert.equal(r.ok, false, riga(valori))
    assert.match(r.motivi.join('; '), motivo)
  }
  // contatore non letto (regole mancanti: rete.sh stampa "?") = rosso, mai verde
  assert.equal(verifica('violazioni_ipv4=? violazioni_ipv6=0 pacchetti_verso_relay=5 pacchetti_loopback=5').ok, false)
  assert.equal(verifica('').ok, false)
})

const sudo = process.platform === 'linux' && spawnSync('sudo', ['-n', 'true']).status === 0
const strumenti = sudo && ['iptables', 'ip6tables', 'ipset', 'setfacl', 'useradd']
  .every(c => spawnSync('sudo', ['-n', 'sh', '-c', `command -v ${c}`]).status === 0)

// in CI gli strumenti mancanti sono un errore, non un salto: un salto lì sarebbe un verde finto
test('rete.sh: regole vere, il controllo positivo passa e le violazioni si contano', {
  skip: !sudo ? 'serve Linux con sudo senza password (CI)' : !strumenti && !process.env.CI ? 'mancano iptables/ipset/acl' : false,
  timeout: 120000,
}, (t) => {
  assert.ok(strumenti, 'mancano iptables/ipset/acl/useradd sul runner')
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'm4i01-'))
  const ipset = path.join(dir, 'm4relay.ipset')
  // una sola destinazione ammessa, di documentazione (RFC 5737): diversa da quella del controllo (192.0.2.1)
  fs.writeFileSync(ipset, 'add m4relay 198.51.100.7,tcp:9001\n')
  const rete = (/** @type {string[]} */ ...a) => spawnSync('sudo', ['-n', 'bash', RETE, ...a], { encoding: 'utf8', timeout: 60000 })
  t.after(() => { rete('pulisci'); fs.rmSync(dir, { recursive: true, force: true }) })

  const p = rete('prepara', ipset)
  assert.equal(p.status, 0, `prepara: ${p.stdout}${p.stderr}`)
  const c = rete('controllo')
  assert.equal(c.status, 0, `controllo positivo caduto: ${c.stdout}${c.stderr}`)
  assert.match(c.stdout, /controllo positivo: risoluzione \(\d+ → \d+\) e host esterno \(\d+ → \d+\)/)
  // dopo il controllo i contatori sono a zero; una violazione nuova deve vedersi in `conta`
  spawnSync('sudo', ['-n', '-u', 'm4app', 'timeout', '5', 'bash', '-c', 'exec 3<>/dev/tcp/192.0.2.1/443'])
  const n = rete('conta')
  assert.equal(n.status, 0, n.stderr)
  const r = verifica(n.stdout)
  assert.ok(r.valori.violazioni_ipv4 > 0, `violazione non contata: ${n.stdout}`)
})
