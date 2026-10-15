// @ts-check
'use strict'
/**
 * Connessioni in uscita SOLO verso servizi onion v3, attraverso il SOCKS5 di Tor.
 * Portato dal nodo v2 (audit C2). Non esiste un modo per aprire una connessione
 * diretta a un host in chiaro da questo modulo.
 */
const net = require('net')
const http = require('http')

/** @param {any} h */
function isOnionV3(h) {
  return typeof h === 'string' && /^[a-z2-7]{56}\.onion$/.test(h)
}

/** @param {number} socksPort @param {string} hostname @param {number} port */
function socks5Connect(socksPort, hostname, port, timeoutMs = 90000) {
  return new Promise((resolve, reject) => {
    if (!isOnionV3(hostname)) return reject(new Error(`solo destinazioni .onion v3 via Tor: ${String(hostname).slice(0, 80)}`))
    const sock = net.createConnection(socksPort, '127.0.0.1')
    let stage = 0, buf = Buffer.alloc(0)
    const fail = (/** @type {Error} */ err) => { sock.removeAllListeners('data'); sock.destroy(); reject(err) }
    sock.setTimeout(timeoutMs)
    sock.once('error', fail)
    sock.once('timeout', () => fail(new Error('SOCKS5: tempo scaduto')))
    sock.on('data', chunk => {
      buf = Buffer.concat([buf, chunk])
      if (stage === 0) {
        if (buf.length < 2) return
        if (buf[0] !== 0x05 || buf[1] !== 0x00) return fail(new Error('SOCKS5: autenticazione rifiutata'))
        buf = buf.subarray(2); stage = 1
        const host = Buffer.from(hostname, 'ascii')
        const req = Buffer.alloc(7 + host.length)
        req[0] = 0x05; req[1] = 0x01; req[2] = 0x00; req[3] = 0x03; req[4] = host.length
        host.copy(req, 5); req.writeUInt16BE(port, 5 + host.length)
        sock.write(req)
      }
      if (stage === 1) {
        if (buf.length < 5) return
        if (buf[1] !== 0x00) return fail(new Error(`SOCKS5: connessione fallita (${buf[1]})`))
        const need = buf[3] === 0x04 ? 22 : buf[3] === 0x03 ? 7 + buf[4] : 10
        if (buf.length < need) return
        sock.removeAllListeners('data'); sock.removeAllListeners('timeout'); sock.removeListener('error', fail)
        sock.setTimeout(0)
        if (buf.length > need) sock.unshift(buf.subarray(need))
        resolve(sock)
      }
    })
    sock.write(Buffer.from([0x05, 0x01, 0x00]))
  })
}

/** http.Agent che apre ogni connessione via Tor e rifiuta host non-onion. */
class TorAgent extends http.Agent {
  /** @param {number} socksPort */
  constructor(socksPort) { super({ keepAlive: false }); this.socksPort = socksPort }
  /** @param {any} options @param {(err: Error|null, s?: net.Socket) => void} cb */
  createConnection(options, cb) {
    socks5Connect(this.socksPort, options.host || options.hostname, parseInt(options.port, 10) || 80)
      .then(s => cb(null, /** @type {net.Socket} */ (s)), err => cb(err))
    return undefined
  }
}

module.exports = { isOnionV3, socks5Connect, TorAgent }
