// @ts-check
'use strict'
/**
 * Tor gestito dal nodo (ruolo server: si usa il `tor` classico, ADR-0013).
 * - SOCKS e servizio onion su porte locali casuali, cartella dati del nodo
 * - il servizio onion punta al listener "public" (mai a quello del proprietario)
 * - `tor` muore con il nodo (processo figlio, __OwningControllerProcess)
 * - l'indirizzo onion non viene scritto nei log (resta al proprietario, I-30)
 */
const fs = require('fs')
const net = require('net')
const path = require('path')
const { spawn } = require('child_process')

/** Porta TCP libera su loopback. */
function freePort() {
  return new Promise((resolve, reject) => {
    const s = net.createServer()
    s.once('error', reject)
    s.listen(0, '127.0.0.1', () => { const p = /** @type {net.AddressInfo} */ (s.address()).port; s.close(() => resolve(p)) })
  })
}

/**
 * @param {{ bin: string, dataDir: string, ingressPort: number, log: (s: string) => void, timeoutMs?: number }} o
 * @returns {Promise<{ socksPort: number, onion: string, stop: () => void }>}
 */
async function startTor(o) {
  const dir = path.join(o.dataDir, 'tor')
  const hs = path.join(dir, 'hs')
  fs.mkdirSync(hs, { recursive: true, mode: 0o700 })
  try { fs.chmodSync(dir, 0o700); fs.chmodSync(hs, 0o700) } catch {}
  const socksPort = /** @type {number} */ (await freePort())
  const torrc = [
    `SocksPort 127.0.0.1:${socksPort}`,
    `DataDirectory ${dir}`,
    `HiddenServiceDir ${hs}`,
    `HiddenServicePort 80 127.0.0.1:${o.ingressPort}`,
    `__OwningControllerProcess ${process.pid}`,
    'Log notice stdout',
    'AvoidDiskWrites 1',
  ].join('\n') + '\n'
  const torrcPath = path.join(dir, 'torrc')
  fs.writeFileSync(torrcPath, torrc, { mode: 0o600 })

  const child = spawn(o.bin, ['-f', torrcPath], { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true })
  let ready = false
  const stop = () => { try { child.kill() } catch {} }
  await new Promise((resolve, reject) => {
    const t = setTimeout(() => { stop(); reject(new Error('Tor: avvio non completato in tempo')) }, o.timeoutMs || 180000)
    child.once('error', e => { clearTimeout(t); reject(new Error(`Tor non avviato: ${e.message}`)) })
    child.once('exit', code => { if (!ready) { clearTimeout(t); reject(new Error(`Tor terminato durante l'avvio (codice ${code})`)) } })
    child.stdout.on('data', d => {
      const s = d.toString()
      const m = /Bootstrapped (\d+)%/.exec(s)
      if (m) o.log(`tor: avvio ${m[1]}%`)
      if (!ready && /Bootstrapped 100%/.test(s)) { ready = true; clearTimeout(t); resolve(undefined) }
    })
    child.stderr.on('data', () => {})
  })
  const hostFile = path.join(hs, 'hostname')
  for (let i = 0; i < 60 && !fs.existsSync(hostFile); i++) await new Promise(r => setTimeout(r, 250))
  const onion = fs.readFileSync(hostFile, 'utf8').trim()
  o.log('tor: pronto, servizio onion attivo')
  return { socksPort, onion, stop }
}

module.exports = { startTor, freePort }
