// @ts-check
'use strict'
/**
 * Directory dei nodi (ADR-0003): schede firmate pubblicate sulle 3 ancore.
 *
 * Lato ANCORA — regole di ingresso (I-29, I-38, I-39):
 *  - accetta solo schede di directory valide (firma, onion v3, nessun IP, scadenza ≤ 7 giorni)
 *  - limite per chiave (intervallo minimo tra aggiornamenti) e tetto di schede
 *    attive: dietro Tor tutti sono 127.0.0.1, quindi i limiti per IP non servono
 *  - prova di lavoro (NIP-13) ADATTIVA sulla prima scheda di una chiave nuova:
 *    più chiavi nuove nell'ultima ora, più bit richiesti (+2 bit a ogni raddoppio
 *    oltre l'obiettivo). Nessun tetto fisso di chiavi nuove: un attaccante non può
 *    "consumare" i posti, può solo pagare una prova sempre più cara; chi la paga entra.
 *  - le schede che arrivano dalle altre ancore saltano i limiti di frequenza (la
 *    replica sposta molte schede insieme) ma non la verifica né il tetto
 *
 * Lato CLIENT — lettura da tutte le ancore e unione (l'ultima scheda valida per
 * nodo vince): un'ancora che nasconde un nodo è compensata dalle altre.
 *
 * Lato NODO — pubblica la propria scheda e la rinnova prima della scadenza.
 */
const WebSocket = require('ws')
const protocol = require('../../packages/protocol')
const { isOnionV3, TorAgent } = require('./socks')

const DEFAULT_LIMITS = Object.freeze({
  powBitsNewKey: 18,          // difficoltà base: ~1 s su un PC, una volta sola per nodo
  newKeysPerHourTarget: 20,   // oltre questo ritmo la difficoltà sale
  powBitsMax: 28,             // tetto alla difficoltà (~4 min su un PC)
  minUpdateIntervalSec: 600,  // il rinnovo normale è ogni ~24 h
  maxActive: 10000,
  peerNewKeysPerHour: 500,    // chiavi nuove accettate in un'ora da UNA ancora-peer (I-39)
})

/**
 * Bit di prova di lavoro richiesti a una chiave nuova, dato quante chiavi nuove
 * l'ancora ha accettato nell'ultima ora. +2 bit (costo ×4) a ogni raddoppio oltre l'obiettivo.
 * @param {number} recentNewKeys @param {{ powBitsNewKey: number, newKeysPerHourTarget: number, powBitsMax: number }} L
 */
function requiredPowBits(recentNewKeys, L) {
  const target = Math.max(1, L.newKeysPerHourTarget)
  const extra = recentNewKeys < target ? 0 : 2 * (1 + Math.floor(Math.log2(recentNewKeys / target)))
  return Math.min(L.powBitsMax, L.powBitsNewKey + extra)
}
const nowSec = () => Math.floor(Date.now() / 1000)

/**
 * Regole di ingresso dell'ancora, da passare al relay come `policy`.
 * @param {{ db: import('node:sqlite').DatabaseSync, network: string, limits?: Partial<typeof DEFAULT_LIMITS> }} o
 * @returns {import('./relay').RelayPolicy}
 */
function anchorPolicy(o) {
  const L = { ...DEFAULT_LIMITS, ...(o.limits || {}) }
  const getKey = o.db.prepare('SELECT first_seen, last_update FROM directory_keys WHERE pubkey = ?')
  const upsert = o.db.prepare('INSERT INTO directory_keys (pubkey, first_seen, last_update) VALUES (?, ?, ?) ON CONFLICT(pubkey) DO UPDATE SET last_update = excluded.last_update')
  const newSince = o.db.prepare('SELECT COUNT(*) AS n FROM directory_keys WHERE first_seen > ?')
  const active = o.db.prepare('SELECT COUNT(*) AS n FROM events WHERE kind = ? AND (expires_at IS NULL OR expires_at > ?)')
  const hasActive = o.db.prepare('SELECT 1 AS x FROM events WHERE kind = ? AND pubkey = ? AND (expires_at IS NULL OR expires_at > ?)')
  /** @type {Map<string, number[]>} ancora-peer → momenti in cui ci ha portato una chiave nuova */
  const peerBudget = new Map()

  return (ev, meta) => {
    if (ev.kind !== protocol.DESCRIPTOR_KIND) return [false, 'blocked: questa ancora accetta solo schede di directory']
    const now = nowSec()
    const v = protocol.verifyDescriptor(ev, o.network, now)
    if (!v.ok) return [false, `invalid: ${/** @type {any} */ (v).reason}`]
    const known = /** @type {any} */ (getKey.get(ev.pubkey))
    const replacing = !!hasActive.get(protocol.DESCRIPTOR_KIND, ev.pubkey, now)
    if (!replacing && /** @type {any} */ (active.get(protocol.DESCRIPTOR_KIND, now)).n >= L.maxActive)
      return [false, 'blocked: directory piena']
    if (meta.fromPeer && !known) {
      // Un'ancora compromessa non deve poter riempire le altre: budget per ancora-peer
      const key = meta.peer || '?'
      const recent = (peerBudget.get(key) || []).filter(t => t > now - 3600)
      if (recent.length >= L.peerNewKeysPerHour) { peerBudget.set(key, recent); return [false, 'rate-limited: budget di chiavi nuove di questa ancora-peer esaurito'] }
      recent.push(now); peerBudget.set(key, recent)
    }
    if (!meta.fromPeer) {
      if (known) {
        if (now - known.last_update < L.minUpdateIntervalSec) return [false, 'rate-limited: aggiornamento troppo frequente per questa chiave']
      } else {
        const need = requiredPowBits(/** @type {any} */ (newSince.get(now - 3600)).n, L)
        if (need > 0 && protocol.leadingZeroBits(ev.id) < need)
          return [false, `pow: servono almeno ${need} bit di prova di lavoro per una chiave nuova`]
      }
    }
    upsert.run(ev.pubkey, known ? known.first_seen : now, now)
    return [true, '']
  }
}

/**
 * Indirizzo di un'ancora o di un nodo: solo onion v3 (il loopback solo nei test).
 * @param {string} url @param {boolean} allowLoop
 */
function parseTarget(url, allowLoop) {
  let u
  try { u = new URL(url) } catch { return null }
  const loop = allowLoop && (u.hostname === '127.0.0.1' || u.hostname === 'localhost')
  if (u.protocol !== 'http:' || (!isOnionV3(u.hostname) && !loop)) return null
  return { ws: `ws://${u.host}/relay`, loop, host: loop ? 'localhost' : u.hostname }
}

/**
 * Apre il relay di un'ancora, invia un messaggio, raccoglie le risposte.
 * @param {string} url @param {any[]} msg @param {(m: any[]) => boolean} until
 * @param {{ socksPort?: number|null, allowLoopbackForTests?: boolean, timeoutMs?: number }} o
 * @returns {Promise<any[][]>}
 */
function exchange(url, msg, until, o) {
  return new Promise((resolve, reject) => {
    const t = parseTarget(url, !!o.allowLoopbackForTests)
    if (!t) return reject(new Error('ancora ignorata: solo indirizzi .onion'))
    if (!t.loop && !o.socksPort) return reject(new Error('Tor non disponibile'))
    const agent = t.loop ? undefined : new TorAgent(/** @type {number} */ (o.socksPort))
    const ws = new WebSocket(t.ws, { agent, headers: { Host: t.host }, handshakeTimeout: t.loop ? 5000 : 90000, maxPayload: 4 * 1024 * 1024 })
    const got = /** @type {any[][]} */ ([])
    const timer = setTimeout(() => { ws.terminate(); reject(new Error('tempo scaduto')) }, o.timeoutMs || (t.loop ? 10000 : 120000))
    ws.on('open', () => ws.send(JSON.stringify(msg)))
    ws.on('message', raw => {
      let m; try { m = JSON.parse(raw.toString()) } catch { return }
      got.push(m)
      if (until(m)) { clearTimeout(timer); ws.close(); resolve(got) }
    })
    ws.on('error', e => { clearTimeout(timer); reject(e) })
  })
}

/**
 * Pubblica una scheda su tutte le ancore. Ritorna l'esito per ancora (non lancia).
 * @param {string[]} anchors @param {any} descriptor
 * @param {{ socksPort?: number|null, allowLoopbackForTests?: boolean, network: string }} o
 */
async function publishDescriptor(anchors, descriptor, o) {
  return Promise.all(anchors.map(async a => {
    try {
      const r = await exchange(a, ['EVENT', descriptor], m => m[0] === 'OK' && m[1] === descriptor.id, o)
      const ok = r.at(-1)
      return { ok: !!ok[2], message: String(ok[3] || '') }
    } catch (e) { return { ok: false, message: /** @type {Error} */ (e).message } }
  }))
}

/**
 * Legge la directory da tutte le ancore, verifica ogni scheda e le unisce
 * (per ogni nodo vince la scheda valida più recente). Le ancore irraggiungibili si ignorano.
 * @param {string[]} anchors
 * @param {{ socksPort?: number|null, allowLoopbackForTests?: boolean, network: string, now?: number }} o
 * @returns {Promise<Array<{ pubkey: string, onion: string, roles: string[], version: string, expiresAt: number, createdAt: number, id: string }>>}
 */
async function fetchDirectory(anchors, o) {
  const now = o.now ?? nowSec()
  const results = await Promise.allSettled(anchors.map(a =>
    exchange(a, ['REQ', 'dir', { kinds: [protocol.DESCRIPTOR_KIND], limit: 500 }], m => (m[0] === 'EOSE' || m[0] === 'CLOSED') && m[1] === 'dir', o)))
  /** @type {Map<string, any>} */ const best = new Map()
  for (const r of results) {
    if (r.status !== 'fulfilled') continue
    for (const m of r.value) {
      if (m[0] !== 'EVENT') continue
      const ev = m[2]
      if (!protocol.verifyDescriptor(ev, o.network, now).ok) continue
      const cur = best.get(ev.pubkey)
      if (!cur || ev.created_at > cur.created_at) best.set(ev.pubkey, ev)
    }
  }
  return [...best.values()].map(ev => {
    const body = JSON.parse(ev.content)
    return {
      pubkey: ev.pubkey, id: ev.id, createdAt: ev.created_at,
      onion: ev.tags.find((/** @type {string[]} */ t) => t[0] === 'onion')[1],
      expiresAt: Number(ev.tags.find((/** @type {string[]} */ t) => t[0] === 'expiration')[1]),
      roles: body.roles, version: String(body.version || ''),
    }
  }).sort((a, b) => a.pubkey.localeCompare(b.pubkey))
}

/**
 * Il nodo pubblica la propria scheda sulle ancore e la rinnova prima della scadenza.
 * @param {object} c
 * @param {import('./config').NodeConfig} c.cfg
 * @param {import('../../packages/keys').RoleKey} c.nodeKey
 * @param {() => string | null} c.onion
 * @param {() => number | null} c.socksPort
 * @param {(s: string) => void} c.log
 * @param {string} c.version
 */
function startPublisher(c) {
  const { cfg, log } = c
  let stopped = false, timer = /** @type {NodeJS.Timeout | null} */ (null)
  let registered = false   // la prova di lavoro serve solo alla prima scheda di una chiave nuova
  const POW_CAP = Math.max(cfg.descriptorPowBits, 26)   // oltre questo il nodo non insiste (minuti di calcolo)
  const anchors = cfg.anchors.filter(a => parseTarget(a, cfg.allowLoopbackPeersForTests))
  if (cfg.anchors.length && !anchors.length) log('directory: nessuna ancora valida (solo indirizzi .onion)')

  async function publishOnce() {
    const onion = c.onion() || cfg.descriptorOnionForTests || null
    if (!onion || !anchors.length) return false
    const opts = { network: cfg.network, socksPort: c.socksPort(), allowLoopbackForTests: cfg.allowLoopbackPeersForTests }
    const build = (/** @type {number} */ bits) => protocol.buildDescriptor(c.nodeKey, cfg.network, {
      onion, roles: [cfg.role], version: c.version, ttlSec: cfg.descriptorTtlSec, powBits: bits,
    })
    let res = await publishDescriptor(anchors, build(registered ? 0 : cfg.descriptorPowBits), opts)
    // ancora sotto pressione: dice quanti bit servono; si rifà la prova e si riprova lì
    const asked = res.map(r => Number(/servono almeno (\d+) bit/.exec(r.message)?.[1] || 0))
    const need = Math.max(...asked)
    if (need > 0 && need <= POW_CAP) {
      log(`directory: un'ancora chiede una prova di lavoro di ${need} bit, la calcolo`)
      const retry = anchors.filter((_, i) => asked[i] > 0)
      const again = await publishDescriptor(retry, build(need), opts)
      let j = 0
      res = res.map((r, i) => asked[i] > 0 ? again[j++] : r)
    } else if (need > POW_CAP) {
      log(`directory: prova di lavoro richiesta (${need} bit) oltre il limite del nodo (${POW_CAP}): riprovo più tardi`)
    }
    const okCount = res.filter(r => r.ok).length
    log(`directory: scheda pubblicata su ${okCount}/${anchors.length} ancore`)
    if (okCount > 0) registered = true
    return okCount > 0
  }

  async function loop() {
    if (stopped) return
    let ok = false
    try { ok = await publishOnce() } catch (e) { log(`directory: pubblicazione fallita (${/** @type {Error} */ (e).message})`) }
    if (stopped) return
    // rinnovo regolare; se nessuna ancora ha accettato, si riprova presto
    timer = setTimeout(loop, (ok ? cfg.descriptorRenewSec : Math.min(60, cfg.descriptorRenewSec)) * 1000)
  }
  if (anchors.length) loop()
  return { stop() { stopped = true; if (timer) clearTimeout(timer) }, publishOnce }
}

module.exports = { DEFAULT_LIMITS, requiredPowBits, anchorPolicy, parseTarget, publishDescriptor, fetchDirectory, startPublisher }
