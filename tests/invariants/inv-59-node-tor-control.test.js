// @ts-check
'use strict'
/**
 * I-59 — Porta di controllo del tor del NODO (aggiunta il 16/10/2026 per seguire gli
 * eventi HS_DESC): solo 127.0.0.1, solo autenticazione a cookie (niente password, niente
 * accesso libero), e negli eventi HS_DESC registrati non compaiono mai indirizzi onion né
 * impronte dei relay (I-30: l'onion del nodo resta al proprietario).
 *
 * Senza tor: il torrc vero (buildTorrc) e la traduzione delle righe di evento (hsDescLog).
 * Con tor (M4TR1X_TOR_BIN): cookie obbligatorio, niente fuori da 127.0.0.1, onion mai nei log.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const net = require('net')
const os = require('os')
const { buildTorrc, hsDescLog, creaTracciatore, attendiAnnuncio, chiaveAccecataDalDescrittore, creaRichiestaAttuale, PUBBLICAZIONE_MINIMA } = require('../../node/src/tor')
const { spawnSync } = require('child_process')
const { startTestNode } = require('../helpers/node')

const ESEMPIO = {
  socksPort: 40001, controlPort: 40002,
  dir: path.join(os.tmpdir(), 'm4-esempio', 'tor'),
  cookieFile: path.join(os.tmpdir(), 'm4-esempio', 'tor', 'control.cookie'), ownerPid: 1234,
}

test('torrc: porta di controllo solo su 127.0.0.1, solo cookie, nessuna password', () => {
  const lines = buildTorrc(ESEMPIO).split('\n').map(l => l.trim()).filter(Boolean)
  const ctl = lines.filter(l => /^ControlPort\b/i.test(l))
  assert.deepEqual(ctl, [`ControlPort 127.0.0.1:${ESEMPIO.controlPort}`], `porte di controllo: ${JSON.stringify(ctl)}`)
  assert.ok(lines.includes('CookieAuthentication 1'), 'cookie non obbligatorio')
  assert.ok(lines.includes('CookieAuthFileGroupReadable 0') && !lines.some(l => /^CookieAuthFileGroupReadable\s+1/i.test(l)), 'cookie leggibile dal gruppo')
  assert.ok(lines.some(l => l === `CookieAuthFile ${ESEMPIO.cookieFile}`), 'cookie fuori dalla cartella di tor')
  assert.ok(!lines.some(l => /^HashedControlPassword\b/i.test(l)), 'password di controllo presente')
  assert.ok(!lines.some(l => /^(ControlSocket|ControlListenAddress)\b/i.test(l)), 'altri ingressi di controllo')
  // riscaldamento: nessun servizio onion nel torrc, si crea con ADD_ONION dopo il bootstrap
  assert.ok(!lines.some(l => /^HiddenService/i.test(l)), 'servizio onion nel torrc (deve nascere dopo il bootstrap)')
  // anche SOCKS solo su loopback (già I-02/I-05, ricontrollato qui sul torrc vero)
  assert.deepEqual(lines.filter(l => /^SocksPort\b/i.test(l)), [`SocksPort 127.0.0.1:${ESEMPIO.socksPort}`])
})

test('eventi HS_DESC nel log senza onion né impronte dei relay', () => {
  // valori riconoscibili, costruiti qui (lo scanner I-28 blocca gli onion scritti nei sorgenti)
  const onion = 'q'.repeat(56), fp = 'AB'.repeat(20), descId = 'Zz9'.repeat(14)
  const righe = [
    `650 HS_DESC UPLOAD ${onion} UNKNOWN $${fp}~relay ${descId} HSDIR_INDEX=${'0'.repeat(64)}`,
    `650 HS_DESC UPLOADED ${onion} UNKNOWN $${fp}~relay`,
    `650 HS_DESC REQUESTED ${onion} NO_AUTH $${fp}~relay ${descId}`,
    `650 HS_DESC FAILED ${onion} NO_AUTH $${fp}~relay REASON=NOT_FOUND`,
    `650 HS_DESC RECEIVED ${onion} NO_AUTH $${fp}`,
  ]
  const out = righe.map(r => hsDescLog(r))
  assert.deepEqual(out.map(o => o && o.testo), ['tor: HS_DESC UPLOAD', 'tor: HS_DESC UPLOADED', 'tor: HS_DESC REQUESTED', 'tor: HS_DESC FAILED NOT_FOUND', 'tor: HS_DESC RECEIVED'])
  // l'impronta torna a parte, per abbinare UPLOAD e risposta in memoria: mai nel testo
  assert.equal(out[0]?.hsdir, fp.toUpperCase())
  for (const o of out) for (const segreto of [onion, fp, descId, 'relay', 'HSDIR'])
    assert.ok(!o?.testo.includes(segreto), `nel log: ${o?.testo}`)
  // righe che non sono eventi HS_DESC: niente nel log, tranne il codice delle risposte d'errore
  assert.equal(hsDescLog(`250 OK`), null)
  assert.equal(hsDescLog(`515 Authentication failed: ${onion}`)?.testo, 'tor: porta di controllo, risposta 515')
})

test(`pubblicato solo con almeno ${PUBBLICAZIONE_MINIMA} UPLOADED del periodo ATTUALE, e l'attuale lo dice tor`, () => {
  const hd = (/** @type {number} */ i) => String(i).padStart(2, '0').repeat(20)
  const ATT = 'attuale', PROX = 'prossimo'
  const up = (/** @type {any} */ t, /** @type {string} */ desc, /** @type {number} */ i) => t.evento({ azione: 'UPLOAD', hsdir: hd(i), desc })
  const ok = (/** @type {any} */ t, /** @type {number} */ i) => t.evento({ azione: 'UPLOADED', hsdir: hd(i) })
  const ko = (/** @type {any} */ t, /** @type {number} */ i) => t.evento({ azione: 'FAILED', hsdir: hd(i) })

  // tor carica il PROSSIMO per primo (succede: 1 prova su 3 il 16/10/2026), poi l'attuale
  const t = creaTracciatore()
  for (let i = 10; i < 18; i++) up(t, PROX, i)
  for (let i = 0; i < 8; i++) up(t, ATT, i)
  assert.deepEqual(t.idVisti(), [PROX, ATT])
  // finché tor non dice quale è l'attuale, niente è pubblicato, nemmeno con 8 conferme
  for (let i = 10; i < 18; i++) assert.equal(ok(t, i), false, 'pubblicato senza sapere quale descrittore è l\'attuale')
  assert.equal(t.impostaAttuale(ATT), false)
  // il primo UPLOADED dell'attuale non basta (condizione vecchia)
  assert.equal(ok(t, 0), false)
  for (let i = 1; i < 5; i++) assert.equal(ok(t, i), false, `pubblicato con ${i + 1} UPLOADED`)
  assert.equal(ko(t, 5), false)
  // il 6° UPLOADED basta, anche con 2 UPLOAD ancora senza risposta (tor li lascia appesi davvero)
  assert.equal(ok(t, 6), true, 'periodo attuale: 6 UPLOADED, doveva essere pubblicato')
  assert.deepEqual(t.stato().attuale, { caricamenti: 8, risposte: 7, caricati: 6, falliti: 1 })

  // l'attuale arriva DOPO le conferme: pubblicato appena tor lo dice
  const r = creaTracciatore()
  for (let i = 0; i < 8; i++) { up(r, ATT, i); ok(r, i) }
  assert.equal(r.impostaAttuale(ATT), true)
  // stesso HSDir nei due periodi: la prima risposta è del primo UPLOAD in sospeso
  const s = creaTracciatore()
  s.impostaAttuale(ATT)
  for (let i = 0; i < 6; i++) { up(s, PROX, i); up(s, ATT, i) }
  for (let i = 0; i < 6; i++) assert.equal(ok(s, i), false, 'le prime risposte erano del periodo prossimo')
  for (let i = 0; i < 6; i++) ok(s, i)
  assert.equal(s.stato().pubblicato, true)
  // lo stesso HSDir due volte conta una volta sola; risposte senza UPLOAD non contano
  const v = creaTracciatore()
  v.impostaAttuale(ATT)
  for (let k = 0; k < 6; k++) { up(v, ATT, 0); ok(v, 0) }
  assert.equal(v.stato().attuale?.caricati, 1)
  const w = creaTracciatore()
  for (let i = 0; i < 8; i++) assert.equal(ok(w, i), false)
})

test('chiave accecata letta dal certificato del descrittore (solo byte, nessuna crittografia)', () => {
  // certificato con la struttura di cert-spec: versione 1, tipo 8, scadenza, tipo chiave 1,
  // chiave certificata (32), 1 estensione: lunghezza 32, tipo 4 (signed-with-key), flag 0, chiave accecata
  const accecata = Buffer.alloc(32, 0x5a)
  const der = Buffer.concat([Buffer.from([1, 8, 0, 0, 0, 1, 1]), Buffer.alloc(32, 0x11), Buffer.from([1, 0, 32, 4, 0]), accecata, Buffer.alloc(64, 0x22)])
  const b64 = der.toString('base64').replace(/(.{64})/g, '$1\n')
  const testo = `hs-descriptor 3\ndescriptor-lifetime 180\ndescriptor-signing-key-cert\n-----BEGIN ED25519 CERT-----\n${b64}\n-----END ED25519 CERT-----\nrevision-counter 1\n`
  assert.equal(chiaveAccecataDalDescrittore(testo), accecata.toString('base64').replace(/=+$/, ''))
  assert.equal(chiaveAccecataDalDescrittore('hs-descriptor 3\n'), null)
  // estensione di altro tipo: nessuna chiave
  const altro = Buffer.from(der); altro[42] = 7
  assert.equal(chiaveAccecataDalDescrittore(testo.replace(b64, altro.toString('base64'))), null)
})

/** Descrittore minimo con la chiave accecata dentro (stessa struttura della prova sopra). */
function descrittoreDiProva(/** @type {Buffer} */ accecata) {
  const der = Buffer.concat([Buffer.from([1, 8, 0, 0, 0, 1, 1]), Buffer.alloc(32, 0x11), Buffer.from([1, 0, 32, 4, 0]), accecata, Buffer.alloc(64, 0x22)])
  return `hs-descriptor 3\ndescriptor-signing-key-cert\n-----BEGIN ED25519 CERT-----\n${der.toString('base64')}\n-----END ED25519 CERT-----\n`
}

test('descrittore attuale non leggibile: il nodo richiede dopo pochi secondi, senza aspettare un id nuovo', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const svuota = () => new Promise(r => setImmediate(r))   // lascia finire le promesse (setImmediate non è finto)
  const accecata = Buffer.alloc(32, 0x5a)
  // tor risponde: 552 (non ancora costruito), 250 illeggibile, connessione che si rompe, 551, 551, 551, poi il descrittore
  const risposte = [
    { codice: 552, righe: ['Unknown onion service'] },
    { codice: 250, righe: ['hs/service/desc/id/xxx=', 'hs-descriptor 3'] },
    'errore',
    { codice: 551, righe: [] }, { codice: 551, righe: [] }, { codice: 551, righe: [] },
    { codice: 250, righe: [descrittoreDiProva(accecata)] },
  ]
  /** @type {string[]} */ const comandi = [], log = [], trovati = []
  const r = creaRichiestaAttuale({
    comando: async cmd => { comandi.push(cmd); const x = risposte.shift(); if (x === 'errore' || !x) throw new Error('connessione rotta'); return /** @type {any} */ (x) },
    host: () => 'host-di-prova', log: s => log.push(s), trovato: id => trovati.push(id),
  })
  r.chiedi()
  /** avanza di ms e verifica quante domande sono partite in tutto */
  const dopo = async (/** @type {number} */ ms, /** @type {number} */ attese) => {
    t.mock.timers.tick(ms); await svuota(); await svuota()
    assert.equal(comandi.length, attese, `dopo altri ${ms} ms: ${comandi.length} domande invece di ${attese}`)
  }
  await dopo(1499, 0)
  await dopo(1, 1)              // prima domanda a 1,5 s dall'id nuovo
  await dopo(4999, 1); await dopo(1, 2)    // 552 → riprova a 5 s
  await dopo(9999, 2); await dopo(1, 3)    // 250 illeggibile → 10 s
  await dopo(19999, 3); await dopo(1, 4)   // connessione rotta → 20 s
  await dopo(40000, 5)                      // 551 → 40 s
  await dopo(60000, 6)                      // tetto 60 s (non 80)
  await dopo(59999, 6); await dopo(1, 7)    // ancora 60 s, poi il descrittore
  assert.deepEqual(trovati, [accecata.toString('base64').replace(/=+$/, '')])
  assert.ok(comandi.every(c => c === 'GETINFO hs/service/desc/id/host-di-prova'))
  assert.equal(log.filter(l => l.startsWith('tor: descrittore attuale non leggibile, riprovo tra')).length, 6, log.join('\n'))
  assert.ok(log.at(-1) === 'tor: descrittore attuale individuato')
  // letto: nessun'altra domanda finché non arriva un id nuovo
  await dopo(600000, 7)

  // spenta (connessione chiusa o nodo fermo): niente più domande, nemmeno quelle già in coda
  const comandi2 = /** @type {string[]} */ ([])
  const r2 = creaRichiestaAttuale({ comando: async c => { comandi2.push(c); return { codice: 552, righe: [] } }, host: () => 'h', log: () => {}, trovato: () => {} })
  r2.chiedi(); t.mock.timers.tick(1500); await svuota(); await svuota()
  assert.equal(comandi2.length, 1)
  r2.ferma(); t.mock.timers.tick(600000); await svuota()
  assert.equal(comandi2.length, 1, 'domande a tor dopo ferma()')
  r2.chiedi(); t.mock.timers.tick(600000); await svuota()
  assert.equal(comandi2.length, 1, 'chiedi() dopo ferma() ha riacceso le domande')

  // chiusura mentre una domanda è già partita: la risposta tardiva non rimette in coda niente
  /** @type {(v: any) => void} */ let rispondi = () => {}
  const comandi3 = /** @type {string[]} */ ([]), log3 = /** @type {string[]} */ ([])
  const r3 = creaRichiestaAttuale({ comando: c => { comandi3.push(c); return new Promise(res => { rispondi = res }) }, host: () => 'h', log: s => log3.push(s), trovato: () => {} })
  r3.chiedi(); t.mock.timers.tick(1500); await svuota()
  assert.equal(comandi3.length, 1)
  r3.ferma(); rispondi({ codice: 552, righe: [] }); await svuota(); await svuota()
  t.mock.timers.tick(600000); await svuota()
  assert.equal(comandi3.length, 1, 'domanda a tor dopo una risposta arrivata a connessione chiusa')
  assert.deepEqual(log3, [], 'riga di nuovo tentativo scritta dopo la chiusura')
})

test('la scheda si annuncia comunque dopo il tempo massimo (un nodo non resta invisibile)', async () => {
  assert.equal(await attendiAnnuncio(new Promise(() => {}), 50).esito, 'scadenza')
  assert.equal(await attendiAnnuncio(Promise.resolve(), 5000).esito, 'confermata')
  // annullata (nodo chiuso): nessun annuncio dopo
  const a = attendiAnnuncio(new Promise(() => {}), 50)
  a.annulla()
  assert.equal(await Promise.race([a.esito, new Promise(r => setTimeout(() => r('ferma'), 150))]), 'ferma')
  // e il nodo la usa davvero, con il tempo dalla configurazione (predefinito 300 s)
  const src = fs.readFileSync(path.join(__dirname, '..', '..', 'node', 'src', 'node.js'), 'utf8')
  assert.match(src, /attendiAnnuncio\(tor\.published, cfg\.announceFallbackSec \* 1000\)/)
  assert.match(src, /attesaAnnuncio\?\.annulla\(\)/, "la chiusura del nodo non annulla l'attesa")
  assert.equal(require('../../node/src/config').normalize({ dataDir: os.tmpdir(), network: 'm4tr1x-test' }).announceFallbackSec, 300)
})

/** SID che non devono avere accesso al cookie su Windows (per SID: i nomi cambiano con la lingua). */
const SID_VIETATI = { 'S-1-1-0': 'Everyone', 'S-1-5-32-545': 'Utenti', 'S-1-5-11': 'Utenti autenticati' }

function cookieSoloUtente(/** @type {string} */ file) {
  if (process.platform !== 'win32') {
    const mode = fs.statSync(file).mode & 0o777
    assert.equal(mode & 0o077, 0, `cookie con permessi ${mode.toString(8)} (gruppo o altri possono leggerlo)`)
    return
  }
  const r = spawnSync('powershell', ['-NoProfile', '-Command',
    `(Get-Acl -LiteralPath '${file.replace(/'/g, "''")}').Access | ForEach-Object { $_.IdentityReference.Translate([System.Security.Principal.SecurityIdentifier]).Value }`], { encoding: 'utf8' })
  assert.equal(r.status, 0, r.stderr)
  const sids = r.stdout.split(/\r?\n/).map(x => x.trim()).filter(Boolean)
  assert.ok(sids.length > 0, 'permessi del cookie non letti')
  for (const s of sids) assert.ok(!(s in SID_VIETATI), `il cookie è leggibile da ${SID_VIETATI[/** @type {keyof typeof SID_VIETATI} */ (s)]} (${s})`)
}

const TOR = process.env.M4TR1X_TOR_BIN
test('con tor vero: cookie obbligatorio, niente fuori da 127.0.0.1, onion mai nei log', {
  skip: !TOR || !fs.existsSync(TOR) ? 'M4TR1X_TOR_BIN non impostato: prove con tor vero saltate' : false, timeout: 600000,
}, async (t) => {
  const n = await startTestNode({ tor: true, torBin: TOR })
  try {
    const torrc = fs.readFileSync(path.join(n.dataDir, 'tor', 'torrc'), 'utf8')
    const port = Number(/^ControlPort 127\.0\.0\.1:(\d+)$/m.exec(torrc)?.[1])
    assert.ok(port > 0, 'porta di controllo non trovata nel torrc')
    // senza cookie: rifiutato
    const risposta = await new Promise((resolve, reject) => {
      const s = net.connect(port, '127.0.0.1', () => s.write('AUTHENTICATE 00\r\nGETINFO version\r\n'))
      let buf = ''
      s.on('data', d => { buf += d.toString(); if (buf.includes('\r\n')) { s.destroy(); resolve(buf) } })
      s.on('error', reject)
      setTimeout(() => { s.destroy(); resolve(buf) }, 10000)
    })
    assert.match(String(risposta), /^515 /, `controllo senza cookie: ${String(risposta).slice(0, 3)}`)
    // il cookie lo legge solo l'utente del nodo
    cookieSoloUtente(path.join(n.dataDir, 'tor', 'control.cookie'))
    // fuori da 127.0.0.1: niente
    const ip = Object.values(os.networkInterfaces()).flat().find(i => i && i.family === 'IPv4' && !i.internal)?.address
    if (ip) {
      const ok = await new Promise(resolve => {
        const s = net.connect(port, ip, () => { s.destroy(); resolve(true) })
        s.on('error', () => resolve(false))
        setTimeout(() => { s.destroy(); resolve(false) }, 3000)
      })
      assert.equal(ok, false, 'porta di controllo raggiungibile da un\'interfaccia di rete')
    }
    // NOSTRO CODICE (rigido): appena tor dice qual è il descrittore attuale (secondi), il nodo
    // deve averlo scelto uguale. RETE TOR (misurata, non rigida): quante conferme arrivano.
    const t4 = /** @type {any} */ (n.node._torForTests)
    const noto = await Promise.race([t4.attualeNoto.then(() => true), new Promise(r => setTimeout(() => r(false), 120000))])
    // MISURA: se fallisce, le ultime righe del nodo dicono se tor non ha caricato (rete)
    // o se GETINFO è stato chiesto e non letto (codice). I log non contengono l'onion (provato sotto).
    assert.ok(noto, 'il nodo non ha chiesto a tor quale descrittore è l\'attuale entro 120 s dai primi caricamenti\n' +
      `ultime righe del nodo:\n${n.logs.filter(l => !l.includes(String(n.node.onion).replace(/\.onion$/, ''))).slice(-40).join('\n')}`)
    const host = String(n.node.onion).replace(/\.onion$/, '')
    // il descrittore che il nodo conta come ATTUALE è quello che tor dice attuale:
    // lo chiediamo qui, per conto nostro, con lo stesso cookie (stesso utente del nodo)
    const cookie = fs.readFileSync(t4.cookieFile).toString('hex')
    const desc = await new Promise((resolve, reject) => {
      const s = net.connect(t4.controlPort, '127.0.0.1', () => s.write(`AUTHENTICATE ${cookie}\r\nGETINFO hs/service/desc/id/${host}\r\n`))
      let buf = ''
      s.on('data', d => { buf += d.toString(); if (/\r\n\.\r\n250 OK\r\n/.test(buf) || /^55\d /m.test(buf)) { s.destroy(); resolve(buf) } })
      s.on('error', reject)
      setTimeout(() => { s.destroy(); resolve(buf) }, 15000)
    })
    const secondoTor = chiaveAccecataDalDescrittore(String(desc))
    assert.ok(secondoTor, 'tor non ha restituito il descrittore attuale')
    assert.equal(t4.stato().idAttuale, secondoTor, 'il nodo ha contato un descrittore diverso da quello che tor dice attuale')
    // rete Tor: si misura fino alla decisione di annunciarsi (conferma o riserva a 5 minuti)
    const t0 = Date.now()
    const decisione = await n.node.announceDecision
    const st = t4.stato()
    t.diagnostic(`annuncio: ${decisione} dopo ${Math.round((Date.now() - t0) / 1000)} s · periodo attuale ${st.attuale?.caricati}/${st.attuale?.caricamenti} UPLOADED · gruppi ${st.gruppi.join(' ')}`)
    // coerenza (nostro codice): "confermata" solo con almeno 6 UPLOADED del periodo attuale
    if (decisione === 'confermata') assert.ok((st.attuale?.caricati ?? 0) >= PUBBLICAZIONE_MINIMA, `confermata con ${st.attuale?.caricati} UPLOADED`)
    else assert.ok(!st.pubblicato, 'scadenza con la pubblicazione già confermata')
    assert.ok(n.logs.some(l => l.startsWith('tor: HS_DESC')), 'nessun evento HS_DESC nel log')
    // K6: la chiave dell'onion sta nel vault cifrato, non in chiaro nella cartella di tor
    const torDir = path.join(n.dataDir, 'tor')
    assert.ok(!fs.existsSync(path.join(torDir, 'hs')), 'cartella hs/ con la chiave in chiaro')
    const vault = fs.readFileSync(path.join(n.dataDir, 'secrets.vault.json'), 'utf8')
    assert.ok(!/ED25519-V3|PrivateKey/.test(vault), "chiave dell'onion leggibile nel vault")
    for (const l of n.logs) assert.ok(!l.includes(host), 'onion del nodo nel log')
  } finally { await n.close() }
})

test('con tor vero: K6 nel vault, al riavvio lo stesso onion', {
  skip: !TOR || !fs.existsSync(TOR) ? 'M4TR1X_TOR_BIN non impostato: prove con tor vero saltate' : false, timeout: 600000,
}, async () => {
  const a = await startTestNode({ tor: true, torBin: TOR })
  const primo = a.node.onion
  const dataDir = a.dataDir
  await a.close()
  assert.ok(!fs.existsSync(path.join(dataDir, 'tor', 'hs')), 'chiave dell\'onion in chiaro nella cartella di tor')
  const b = await startTestNode({ tor: true, torBin: TOR, dataDir })
  try {
    assert.equal(b.node.onion, primo, 'al riavvio l\'onion è cambiato: la chiave non è stata conservata')
    assert.ok(!b.logs.some(l => l.includes(primo.replace(/\.onion$/, ''))), 'onion del nodo nel log')
  } finally { await b.close() }
})

test('con tor vero: connessione di controllo che cade → riconnessione e stesso onion ricreato da solo', {
  skip: !TOR || !fs.existsSync(TOR) ? 'M4TR1X_TOR_BIN non impostato: prove con tor vero saltate' : false, timeout: 600000,
}, async () => {
  const n = await startTestNode({ tor: true, torBin: TOR })
  try {
    const onion = n.node.onion
    const t4 = /** @type {any} */ (n.node._torForTests)
    const prima = n.logs.length
    t4.chiudiControllo()   // guasto simulato: la connessione si chiude, il nodo resta acceso
    const attendi = async (/** @type {(l: string) => boolean} */ f, /** @type {number} */ ms, /** @type {number} */ da) => {
      const fine = Date.now() + ms
      while (Date.now() < fine) { const i = n.logs.slice(da).findIndex(f); if (i >= 0) return da + i; await new Promise(r => setTimeout(r, 250)) }
      return -1
    }
    const persa = await attendi(l => l.startsWith('tor: connessione di controllo persa'), 10000, prima)
    assert.ok(persa >= 0, 'il nodo non si è accorto della connessione persa')
    const ricreato = await attendi(l => l === 'tor: onion ricreato dopo la riconnessione (stesso indirizzo)', 60000, persa)
    assert.ok(ricreato >= 0, `onion non ricreato: ${n.logs.slice(persa).filter(l => /tor:/.test(l)).slice(-5).join(' | ')}`)
    // prova che il servizio è davvero rinato: tor ne ricarica il descrittore sulla connessione nuova
    const upload = await attendi(l => l === 'tor: HS_DESC UPLOAD', 120000, ricreato)
    assert.ok(upload >= 0, 'nessun nuovo caricamento del descrittore dopo la riconnessione')
    assert.equal(n.node.onion, onion, 'indirizzo cambiato dopo la riconnessione')
  } finally { await n.close() }
})

test('con tor vero: se tor muore il nodo si chiude con errore (lo riavvia il sistema)', {
  skip: !TOR || !fs.existsSync(TOR) ? 'M4TR1X_TOR_BIN non impostato: prove con tor vero saltate' : false, timeout: 600000,
}, async () => {
  /** @type {Error|null} */ let fatale = null
  const n = await startTestNode({ tor: true, torBin: TOR, onFatal: (/** @type {Error} */ e) => { fatale = e } })
  const port = n.node.port
  const pid = /** @type {any} */ (n.node._torForTests).pid
  assert.ok(pid > 0, 'pid di tor sconosciuto')
  process.kill(pid)   // tor muore con il nodo acceso
  const fine = Date.now() + 30000
  while (!fatale && Date.now() < fine) await new Promise(r => setTimeout(r, 250))
  assert.ok(fatale, 'onFatal non chiamato: il nodo resta acceso senza tor')
  assert.match(String(/** @type {any} */ (fatale).message), /tor/)
  assert.ok(n.logs.some(l => /tor: ERRORE, processo tor terminato/.test(l)), 'errore non registrato nel log')
  // il nodo si è chiuso davvero: la sua porta non risponde più
  const risponde = await new Promise(resolve => {
    const s = net.connect(port, '127.0.0.1', () => { s.destroy(); resolve(true) })
    s.on('error', () => resolve(false))
  })
  assert.equal(risponde, false, 'il nodo ascolta ancora dopo la morte di tor')
})

/** tor ancora vivi con quella cartella dati (per riga di comando del processo). */
function torViviIn(/** @type {string} */ dir) {
  const r = process.platform === 'win32'
    ? spawnSync('powershell', ['-NoProfile', '-Command', "Get-CimInstance Win32_Process -Filter \"Name='tor.exe'\" | ForEach-Object { $_.CommandLine }"], { encoding: 'utf8' })
    : spawnSync('ps', ['-eo', 'args'], { encoding: 'utf8' })
  return (r.stdout || '').split(/\r?\n/).filter(l => /tor/.test(l) && l.includes(dir)).length
}

test('con tor vero: bootstrap non completato in tempo → il nodo non parte e chiude tutto', {
  skip: !TOR || !fs.existsSync(TOR) ? 'M4TR1X_TOR_BIN non impostato: prove con tor vero saltate' : false, timeout: 120000,
}, async () => {
  const port = await new Promise(resolve => { const s = net.createServer().listen(0, '127.0.0.1', () => { const p = /** @type {any} */ (s.address()).port; s.close(() => resolve(p)) }) })
  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'm4-bootstrap-'))
  // 1 secondo: nessun tor arriva al 100% così in fretta (nemmeno con la cache calda)
  await assert.rejects(startTestNode({ tor: true, torBin: TOR, dataDir, port, torBootstrapTimeoutSec: 1 }), /avvio non completato in tempo/)
  const risponde = await new Promise(resolve => {
    const s = net.connect(/** @type {number} */ (port), '127.0.0.1', () => { s.destroy(); resolve(true) })
    s.on('error', () => resolve(false))
  })
  assert.equal(risponde, false, 'il nodo ascolta ancora dopo l\'avvio fallito')
  await new Promise(r => setTimeout(r, 2000))
  assert.equal(torViviIn(dataDir), 0, 'tor ancora acceso dopo l\'avvio fallito')
})
