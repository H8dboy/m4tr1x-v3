// @ts-check
'use strict'
/**
 * Accettazione F2 — due nodi si scambiano eventi e blob.
 * (1) in locale (peer su loopback, solo in modalità test);
 * (2) via Tor vero: parte solo se M4TR1X_TOR_BIN punta a un binario `tor`
 *     (in CI lo installa il job "tor"); altrimenti viene saltata e lo dice.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const crypto = require('crypto')
const fs = require('fs')
const protocol = require('../../packages/protocol')
const { NET, TOR_CACHE_DIR, TOR_CACHE_FILES, startTestNode, request, wsOpen, wsExchange, testIdentity, uploadAuth } = require('../helpers/node')

async function waitFor(fn, ms = 20000, step = 250) {
  const end = Date.now() + ms
  while (Date.now() < end) { const v = await fn(); if (v) return v; await new Promise(r => setTimeout(r, step)) }
  return null
}

/** In locale le fasi non si misurano: `fase` esegue e basta. */
const senzaFasi = (/** @type {string} */ _nome, /** @type {() => Promise<any>} */ fn) => fn()

async function exchangeScenario(a, b, owner, fase = senzaFasi) {
  // un file caricato su A e un evento su A che lo cita
  const data = crypto.randomBytes(20000)
  const auth = uploadAuth(owner, data)
  const up = await request(a.node.port, '/blob', { method: 'PUT', body: data, headers: { Authorization: auth.header } })
  assert.equal(up.status, 201, up.body.toString())
  const ev = protocol.buildEvent({ kind: 1, content: 'testimonianza di prova', tags: [['x', auth.sha]] }, owner, NET)
  const wsA = await wsOpen(a.node.port)
  const ok = await wsExchange(wsA, ['EVENT', ev], m => m[0] === 'OK')
  assert.equal(ok.at(-1)[2], true, JSON.stringify(ok.at(-1)))
  wsA.close()

  // B riceve l'evento dal peer…
  await fase('evento', async () => {
    const gotEvent = await waitFor(async () => {
      const ws = await wsOpen(b.node.port)
      const msgs = await wsExchange(ws, ['REQ', 'q', { ids: [ev.id] }], m => m[0] === 'EOSE')
      ws.close()
      return msgs.find(m => m[0] === 'EVENT')
    }, 60000)
    assert.ok(gotEvent, 'B non ha ricevuto l\'evento da A')
  })
  // …e il file citato, verificato per hash
  await fase('file', async () => {
    const gotBlob = await waitFor(async () => {
      const r = await request(b.node.port, '/blob/' + auth.sha)
      return r.status === 200 && r.body.equals(data)
    }, 60000)
    assert.ok(gotBlob, 'B non ha replicato il file da A')
  })
}

test('due nodi in locale si scambiano evento e file', async () => {
  const owner = testIdentity()
  const a = await startTestNode({ uploaders: [owner.publicKey] })
  const b = await startTestNode({ peers: [`http://127.0.0.1:${a.node.port}`], allowLoopbackPeersForTests: true })
  try { await exchangeScenario(a, b, owner) }
  finally { await b.close(); await a.close() }
})

/**
 * Via Tor la prova è divisa in FASI misurate, con un tempo massimo per fase:
 *   tor-a         A avvia tor e crea il suo onion
 *   pubblicazione A decide di annunciarsi: conferma (almeno 6 HSDir del periodo attuale con
 *                 UPLOADED) oppure riserva dopo 5 minuti, come il nodo vero; si stampa quale
 *   tor-b         B avvia tor: SOLO dopo la pubblicazione (ipotesi del fondatore, 16/10/2026:
 *                 se B cerca l'onion prima, tor si segna il fallimento e aspetta a ritentare)
 *   rete      B raggiunge l'onion di A (pubblicazione del descrittore: la fase lenta)
 *   evento    B riceve l'evento da A          ← il nostro codice
 *   file      B replica il file citato        ← il nostro codice
 * I tempi si stampano a ogni esecuzione (anche in CI, come "# fasi: …"); se una fase
 * supera il suo massimo, l'errore dice QUALE, dopo quanto, e le ultime righe di log di B
 * sul peer. Così un rosso dice di chi è la colpa e non si impara a ignorarlo.
 * Massimi scelti sui tempi MISURATI il 16/10/2026 (5 giri in locale + CI):
 *   tor-a, tor-b  tipici 8–22 s; una volta bloccato oltre 120 s → 200 s, sopra i 180 s dopo
 *                 cui il nodo stesso si arrende dicendo a che percentuale si è fermato tor
 *   rete          tipico ~4 s, peggiore visto 124 s (la vecchia finestra unica di 60 s cadeva qui) → 300 s
 *   evento, file  0,3–1,2 s: il nostro codice, resta stretto → 60 s
 */
const LIMITI_MS = { 'tor-a': 200000, pubblicazione: 330000, 'tor-b': 200000, rete: 300000, evento: 60000, file: 60000 }

const TOR = process.env.M4TR1X_TOR_BIN
test('due nodi via Tor si scambiano evento e file', { skip: !TOR || !fs.existsSync(TOR) ? 'M4TR1X_TOR_BIN non impostato: prova via Tor saltata' : false, timeout: 1300000 }, async (t) => {
  const owner = testIdentity()
  /** @type {Record<string, number>} */ const tempi = {}
  /** @type {any} */ let a = null, b = null
  let finito = false
  /** @type {boolean|null} */ let cacheCalda = null
  /** @type {string} */ let decisioneA = '?'
  // log raccolti dall'inizio (anche se l'avvio del nodo non finisce): ultime righe su tor e peer
  /** @type {string[]} */ const logA = [], logB = []
  const ultime = (/** @type {string[]} */ l) => l.filter(x => /peer|tor/i.test(x)).slice(-10).join(' | ') || '(nessuno)'
  async function fase(/** @type {keyof typeof LIMITI_MS} */ nome, /** @type {() => Promise<any>} */ fn) {
    const t0 = Date.now()
    /** @type {any} */ let timer
    const scade = new Promise((_, rej) => { timer = setTimeout(() => rej(new Error('scaduta')), LIMITI_MS[nome]) })
    try { return await Promise.race([fn(), scade]) }
    catch (e) {
      tempi[nome] = Date.now() - t0
      throw new Error(`fase "${nome}" fallita dopo ${Math.round(tempi[nome] / 1000)} s (massimo ${LIMITI_MS[nome] / 1000} s): ${/** @type {Error} */ (e).message}` +
        ` · tempi finora: ${JSON.stringify(tempi)} · log di A: ${ultime(logA)} · log di B: ${ultime(logB)}`)
    } finally { clearTimeout(timer); tempi[nome] ??= Date.now() - t0 }
  }
  try {
    a = await fase('tor-a', () => startTestNode({ uploaders: [owner.publicKey], tor: true, torBin: TOR, log: l => logA.push(l) }))
    cacheCalda = a.torCacheWarm
    assert.ok(a.node.onion && /^[a-z2-7]{56}\.onion$/.test(a.node.onion), 'A non ha un onion')
    decisioneA = await fase('pubblicazione', () => a.node.announceDecision)
    b = await fase('tor-b', () => startTestNode({ peers: [`http://${a.node.onion}`], tor: true, torBin: TOR, log: l => logB.push(l) }))
    await fase('rete', async () => {
      // il ciclo si ferma anche quando la fase scade (finito), non gira a vuoto fino alla fine
      while (!finito && !logB.some(l => l.includes('peer: collegato'))) await new Promise(r => setTimeout(r, 500))
    })
    await exchangeScenario(a, b, owner, fase)
    await b.close(); await a.close(); b = a = null
    // la cache condivisa contiene solo file pubblici e firmati: mai chiavi né `state`
    if (fs.existsSync(TOR_CACHE_DIR)) for (const f of fs.readdirSync(TOR_CACHE_DIR))
      assert.ok(TOR_CACHE_FILES.includes(f), `file non ammesso nella cache di tor: ${f}`)
  } finally {
    finito = true
    const cache = cacheCalda === null ? '?' : cacheCalda ? 'calda' : 'fredda'
    t.diagnostic(`fasi (cache ${cache}, annuncio di A: ${decisioneA}): ${Object.entries(tempi).map(([k, v]) => `${k}=${(v / 1000).toFixed(1)}s`).join(' ')}`)
    // eventi HS_DESC contati per azione: "A UPLOAD×8 UPLOADED×4 · B REQUESTED×1 RECEIVED×1"
    const conta = (/** @type {string[]} */ l) => Object.entries(l.reduce((/** @type {Record<string, number>} */ m, x) => {
      const e = /^tor: HS_DESC (\S+(?: \S+)?)/.exec(x)?.[1]; if (e) m[e] = (m[e] || 0) + 1; return m }, {})).map(([k, v]) => `${k}×${v}`).join(' ') || '—'
    t.diagnostic(`HS_DESC A: ${conta(logA)} · B: ${conta(logB)}`)
    if (b) await b.close()
    if (a) await a.close()
  }
})
