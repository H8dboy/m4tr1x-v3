// @ts-check
'use strict'
/**
 * Tor gestito dal nodo (ruolo server: si usa il `tor` classico, ADR-0013).
 * - SOCKS e servizio onion su porte locali casuali, cartella dati del nodo
 * - il servizio onion punta al listener "public" (mai a quello del proprietario)
 * - `tor` muore con il nodo (processo figlio, __OwningControllerProcess)
 * - l'indirizzo onion non viene scritto nei log (resta al proprietario, I-30)
 * - porta di controllo (I-59): solo 127.0.0.1, porta casuale, solo cookie leggibile dal
 *   solo utente del nodo; serve a seguire gli eventi HS_DESC, che finiscono nel log con
 *   azione e motivo ma MAI con indirizzi onion o impronte dei relay
 * - "pubblicato" (decisione del fondatore, 16/10/2026): tor carica il descrittore su circa
 *   8 HSDir per periodo e chi cerca ne interroga uno a caso; il primo UPLOADED non basta.
 *   L'onion è pubblicato quando OGNI UPLOAD ha avuto risposta (UPLOADED o FAILED) e almeno
 *   PUBBLICAZIONE_MINIMA HSDir diversi hanno risposto UPLOADED. Vale anche in produzione:
 *   il nodo non annuncia la sua scheda nella directory prima di quel momento (node.js).
 */
const fs = require('fs')
const net = require('net')
const path = require('path')
const { spawn } = require('child_process')

const PUBBLICAZIONE_MINIMA = 6

/** Porta TCP libera su loopback. */
function freePort() {
  return new Promise((resolve, reject) => {
    const s = net.createServer()
    s.once('error', reject)
    s.listen(0, '127.0.0.1', () => { const p = /** @type {net.AddressInfo} */ (s.address()).port; s.close(() => resolve(p)) })
  })
}

/**
 * Il torrc del nodo. Funzione pura: il test di I-59 controlla questo testo, non il sorgente.
 * NIENTE servizio onion qui (riscaldamento, decisione del fondatore del 16/10/2026): tor parte
 * senza onion, arriva al bootstrap completo, e solo dopo il nodo crea l'onion con ADD_ONION.
 * Così il descrittore si pubblica con consenso e microdescrittori già pronti.
 * @param {{ socksPort: number, controlPort: number, dir: string, cookieFile: string, ownerPid: number }} o
 */
function buildTorrc(o) {
  return [
    `SocksPort 127.0.0.1:${o.socksPort}`,
    `DataDirectory ${o.dir}`,
    `__OwningControllerProcess ${o.ownerPid}`,
    `ControlPort 127.0.0.1:${o.controlPort}`,
    'CookieAuthentication 1',
    `CookieAuthFile ${o.cookieFile}`,
    'CookieAuthFileGroupReadable 0',
    'Log notice stdout',
    'AvoidDiskWrites 1',
  ].join('\n') + '\n'
}

/**
 * Una riga della porta di controllo → riga di log (o null). Solo azione e motivo: niente
 * indirizzo onion, niente impronta dell'HSDir, niente id del descrittore. L'impronta torna
 * a parte (`hsdir`) SOLO per abbinare UPLOAD e risposta in memoria: non va mai nel log.
 * @param {string} line
 * @returns {{ testo: string, azione?: string, hsdir?: string, desc?: string } | null}
 */
function hsDescLog(line) {
  // 650 HS_DESC <Azione> <Indirizzo> <Auth> <HSDir> [IdDescrittore] [REASON=..] [..]
  const m = /^650 HS_DESC (\S+) \S+ \S+ (\S+)(?: ([^\s=]+))?/.exec(line)
  if (!m) return /^5\d\d /.test(line) ? { testo: `tor: porta di controllo, risposta ${line.slice(0, 3)}` } : null
  const reason = /REASON=([A-Z_]+)/.exec(line)?.[1]
  const hsdir = /^\$?([0-9A-Fa-f]{40})/.exec(m[2])?.[1]?.toUpperCase()
  // id del descrittore (chiave accecata del periodo): solo in memoria, per distinguere i periodi
  const desc = m[1] === 'UPLOAD' ? m[3] : undefined
  return { testo: `tor: HS_DESC ${m[1]}${reason ? ` ${reason}` : ''}`, azione: m[1], hsdir, desc }
}

/**
 * Abbina ogni UPLOAD alla sua risposta e dice quando l'onion è pubblicato.
 * tor carica due descrittori: periodo ATTUALE e periodo PROSSIMO (~8 HSDir ciascuno). Conta
 * solo l'attuale, l'unico che oggi un client cerca. Gli UPLOAD portano l'id del descrittore
 * (chiave accecata del periodo), le risposte no: si abbinano per HSDir, nell'ordine.
 * Regola (fondatore, 16/10/2026, corretta sui dati): pubblicato quando almeno `minimo` HSDir
 * DIVERSI del periodo attuale hanno risposto UPLOADED. Gli UPLOAD senza risposta NON bloccano:
 * tor ne lascia davvero alcuni appesi per minuti, senza UPLOADED né FAILED (osservato).
 * QUALE gruppo è l'attuale lo dice tor (GETINFO hs/service/desc/id → chiave accecata nel
 * certificato del descrittore, `impostaAttuale`). NON si deduce dall'ordine dei caricamenti:
 * il 16/10/2026 in 1 prova su 3 tor ha caricato prima il descrittore del periodo prossimo.
 * Finché tor non l'ha detto, niente è pubblicato (resta l'annuncio di riserva del nodo).
 * @param {number} [minimo]
 */
function creaTracciatore(minimo = PUBBLICAZIONE_MINIMA) {
  /** @type {Map<string, string[]>} HSDir → id dei descrittori caricati e ancora senza risposta */
  const inAttesa = new Map()
  /** @type {Map<string, { caricamenti: number, risposte: number, caricati: Set<string>, falliti: number }>} */
  const gruppi = new Map()
  /** @type {string|null} */ let attuale = null
  let pubblicato = false
  const gruppo = (/** @type {string} */ id) => {
    let g = gruppi.get(id)
    if (!g) { g = { caricamenti: 0, risposte: 0, caricati: new Set(), falliti: 0 }; gruppi.set(id, g) }
    return g
  }
  const controlla = () => {
    const g = attuale === null ? null : gruppi.get(attuale)
    if (!pubblicato && g && g.caricati.size >= minimo) { pubblicato = true; return true }
    return false
  }
  return {
    /** @param {{ azione?: string, hsdir?: string, desc?: string } | null} ev @returns {boolean} true la prima volta che la condizione è vera */
    evento(ev) {
      if (!ev || !ev.azione || !ev.hsdir) return false
      if (ev.azione === 'UPLOAD') {
        const id = ev.desc || 'unico'
        gruppo(id).caricamenti++
        inAttesa.set(ev.hsdir, [...(inAttesa.get(ev.hsdir) || []), id])
      } else if (ev.azione === 'UPLOADED' || ev.azione === 'FAILED') {
        const coda = inAttesa.get(ev.hsdir)
        if (!coda || !coda.length) return false   // risposta senza UPLOAD (ricerca di un altro onion): non è nostra
        const id = /** @type {string} */ (coda.shift())
        if (!coda.length) inAttesa.delete(ev.hsdir)
        const g = gruppo(id)
        g.risposte++
        if (ev.azione === 'UPLOADED') g.caricati.add(ev.hsdir); else g.falliti++
      } else return false
      return controlla()
    },
    /** Quale descrittore è l'attuale, secondo tor. @param {string} id @returns {boolean} come evento() */
    impostaAttuale(id) {
      attuale = id
      return controlla()
    },
    /** Id dei descrittori caricati finora (per sapere quando chiedere a tor). */
    idVisti() { return [...gruppi.keys()] },
    stato() {
      const g = attuale === null ? null : gruppi.get(attuale)
      return {
        pubblicato,
        /** id del descrittore preso come attuale: SOLO in memoria e per il test di I-59, mai nel log */
        idAttuale: attuale,
        attuale: g ? { caricamenti: g.caricamenti, risposte: g.risposte, caricati: g.caricati.size, falliti: g.falliti } : null,
        gruppi: [...gruppi.values()].map(x => `${x.caricati.size}/${x.caricamenti}`),
      }
    },
  }
}

/**
 * Quando annunciare la scheda nella directory: alla pubblicazione confermata, oppure comunque
 * dopo `ms` (un nodo non deve restare invisibile per sempre se le conferme non arrivano).
 * `annulla()` (alla chiusura del nodo) ferma l'attesa: la promessa resta in sospeso.
 * @param {Promise<void>} published @param {number} ms
 * @returns {{ esito: Promise<'confermata'|'scadenza'>, annulla: () => void }}
 */
function attendiAnnuncio(published, ms) {
  /** @type {any} */ let t
  let annullato = false
  const esito = new Promise(resolve => {
    t = setTimeout(() => { if (!annullato) resolve('scadenza') }, ms)
    published.then(() => { clearTimeout(t); if (!annullato) resolve('confermata') })
  })
  return { esito: /** @type {Promise<'confermata'|'scadenza'>} */ (esito), annulla: () => { annullato = true; clearTimeout(t) } }
}

/**
 * Chiave accecata (id del descrittore, come negli eventi HS_DESC) dal testo di un descrittore
 * v3: è nell'estensione "signed-with-key" (tipo 4) del certificato Ed25519 della chiave di firma.
 * Formato del certificato (cert-spec): versione, tipo, scadenza(4), tipo chiave, chiave(32),
 * numero di estensioni, poi per ognuna lunghezza(2), tipo, flag, dati. Solo lettura di byte:
 * nessuna operazione Ed25519 nel nostro codice (I-40).
 * @param {string} testo @returns {string|null} base64 senza "=" finali
 */
function chiaveAccecataDalDescrittore(testo) {
  const m = /descriptor-signing-key-cert\s*-----BEGIN ED25519 CERT-----\s*([\s\S]*?)-----END ED25519 CERT-----/.exec(testo)
  if (!m) return null
  const der = Buffer.from(m[1].replace(/\s+/g, ''), 'base64')
  let off = 1 + 1 + 4 + 1 + 32
  if (der.length < off + 1) return null
  const n = der[off++]
  for (let k = 0; k < n; k++) {
    if (der.length < off + 4) return null
    const len = der.readUInt16BE(off), tipo = der[off + 2]
    if (tipo === 4 && len === 32 && der.length >= off + 4 + 32) return der.subarray(off + 4, off + 4 + 32).toString('base64').replace(/=+$/, '')
    off += 4 + len
  }
  return null
}

/** Permessi del cookie: solo l'utente del nodo (su Linux/macOS 0600; su Windows lo controlla I-59). */
function restringiCookie(/** @type {string} */ file) {
  try { fs.chmodSync(file, 0o600) } catch {}
}

/**
 * Client minimo della porta di controllo: comandi con risposta (anche su più righe, "250+…"
 * fino a ".") ed eventi asincroni ("650 …") separati. Le risposte non finiscono mai nel log.
 * @param {net.Socket} sock @param {(riga: string) => void} onEvento
 */
function creaControllo(sock, onEvento) {
  /** @type {{ resolve: (r: { codice: number, righe: string[] }) => void }[]} */ const attesa = []
  /** @type {string[]} */ let righe = []
  /** @type {string[]|null} */ let dati = null
  let buf = ''
  sock.on('data', d => {
    buf += d.toString()
    let i
    while ((i = buf.indexOf('\r\n')) >= 0) {
      const line = buf.slice(0, i); buf = buf.slice(i + 2)
      if (dati !== null) { if (line === '.') { righe.push(dati.join('\n')); dati = null } else dati.push(line); continue }
      if (/^650[ -]/.test(line)) { onEvento(line); continue }
      if (/^\d{3}\+/.test(line)) { righe.push(line.slice(4)); dati = []; continue }
      righe.push(line.slice(4))
      if (/^\d{3} /.test(line)) {
        const r = { codice: Number(line.slice(0, 3)), righe }
        righe = []
        attesa.shift()?.resolve(r)
      }
    }
  })
  return {
    /** @param {string} cmd @returns {Promise<{ codice: number, righe: string[] }>} */
    comando(cmd) {
      return new Promise(resolve => { attesa.push({ resolve }); sock.write(`${cmd}\r\n`) })
    },
  }
}

/**
 * Avvio di tor con riscaldamento (decisione del fondatore, 16/10/2026):
 *  1. tor parte SENZA onion e arriva al bootstrap completo (consenso e microdescrittori pronti);
 *  2. il nodo si collega alla porta di controllo (solo 127.0.0.1, cookie) e segue gli HS_DESC;
 *  3. solo ora crea l'onion con ADD_ONION, con la chiave K6 presa dal vault del nodo
 *     (al primo avvio la genera tor, NEW, e il nodo la sigilla subito nel vault). La chiave
 *     passa a tor come blob opaco: nessuna operazione Ed25519 nel nostro codice (I-40).
 * Un onion di ADD_ONION vive quanto la connessione di controllo. Se la connessione cade con
 * il nodo vivo, l'onion sparirebbe in silenzio: il nodo si RICOLLEGA da solo (attese 1, 2, 4…
 * fino a 30 s) e rifà ADD_ONION con la stessa chiave, quindi lo stesso indirizzo.
 * @param {{ bin: string, dataDir: string, ingressPort: number, log: (s: string) => void, timeoutMs?: number,
 *   chiaveOnion?: string|null, salvaChiaveOnion?: (k: string) => void, onMorto?: (codice: number|null) => void }} o
 * @returns {Promise<{ socksPort: number, onion: string, stop: () => void, published: Promise<void>, statoPubblicazione: () => ReturnType<ReturnType<typeof creaTracciatore>['stato']>, attualeNoto: Promise<void>, controlPort: number, cookieFile: string, chiudiControlloPerProva: () => void, pid: number|undefined }>}
 */
async function startTor(o) {
  const dir = path.join(o.dataDir, 'tor')
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 })
  try { fs.chmodSync(dir, 0o700) } catch {}
  if (fs.existsSync(path.join(dir, 'hs'))) o.log("tor: cartella hs/ di una versione precedente ignorata (la chiave dell'onion ora sta nel vault)")
  const socksPort = /** @type {number} */ (await freePort())
  const controlPort = /** @type {number} */ (await freePort())
  const cookieFile = path.join(dir, 'control.cookie')
  const torrc = buildTorrc({ socksPort, controlPort, dir, cookieFile, ownerPid: process.pid })
  const torrcPath = path.join(dir, 'torrc')
  fs.writeFileSync(torrcPath, torrc, { mode: 0o600 })

  const child = spawn(o.bin, ['-f', torrcPath], { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true })
  let ready = false, fermato = false
  /** @type {net.Socket|null} */ let sock = null
  /** @type {NodeJS.Timeout|null} */ let riprova = null
  const stop = () => {
    fermato = true
    if (riprova) clearTimeout(riprova)
    try { sock?.destroy() } catch {}
    try { child.kill() } catch {}
  }
  // 1. bootstrap completo, senza onion
  await new Promise((resolve, reject) => {
    const t = setTimeout(() => { stop(); reject(new Error('Tor: avvio non completato in tempo')) }, o.timeoutMs || 180000)
    child.once('error', e => { clearTimeout(t); reject(new Error(`Tor non avviato: ${e.message}`)) })
    child.once('exit', code => {
      if (!ready) { clearTimeout(t); reject(new Error(`Tor terminato durante l'avvio (codice ${code})`)) }
      else if (!fermato) {
        // niente riavvio qui dentro (decisione del fondatore): il nodo si chiude con errore e lo
        // riavvia il sistema (systemd Restart=on-failure, servizio Windows)
        o.log(`tor: ERRORE, processo tor terminato con il nodo acceso (codice ${code}): onion non raggiungibile`)
        fermato = true
        if (riprova) clearTimeout(riprova)
        o.onMorto?.(code)
      }
    })
    child.stdout.on('data', d => {
      const s = d.toString()
      const m = /Bootstrapped (\d+)%/.exec(s)
      if (m) o.log(`tor: avvio ${m[1]}%`)
      if (!ready && /Bootstrapped 100%/.test(s)) { ready = true; clearTimeout(t); resolve(undefined) }
    })
    child.stderr.on('data', () => {})
  })
  restringiCookie(cookieFile)

  // 2-3. porta di controllo, eventi, onion: in una funzione, perché si rifà a ogni riconnessione
  /** @type {(v?: any) => void} */ let segnaAttualeNoto = () => {}
  const attualeNoto = /** @type {Promise<void>} */ (new Promise(r => { segnaAttualeNoto = r }))
  /** @type {(v?: any) => void} */ let segnaPubblicato = () => {}
  const published = /** @type {Promise<void>} */ (new Promise(r => { segnaPubblicato = r }))
  let tracciatore = creaTracciatore()
  let chiave = o.chiaveOnion || null
  let onion = ''

  async function collega() {
    tracciatore = creaTracciatore()   // tor ricarica i descrittori da capo
    let idNoti = 0
    /** @type {NodeJS.Timeout|null} */ let chiedi = null
    const fatto = () => {
      const s = tracciatore.stato(), a = s.attuale
      o.log(`tor: descrittore onion pubblicato (periodo attuale: ${a?.caricati} HSDir su ${a?.caricamenti}, ${a ? a.caricamenti - a.risposte : 0} ancora senza risposta, ${a?.falliti} falliti; gruppi ${s.gruppi.join(' ')})`)
      segnaPubblicato()
    }
    const cookie = fs.readFileSync(cookieFile).toString('hex')
    const s = await new Promise((resolve, reject) => {
      const c = net.connect(controlPort, '127.0.0.1', () => resolve(c))
      c.once('error', reject)
    })
    sock = /** @type {net.Socket} */ (s)
    /** @type {() => void} */ let chiediAttuale = () => {}
    const ctl = creaControllo(sock, line => {
      const ev = hsDescLog(line)
      if (!ev) return
      o.log(ev.testo)
      const pubblicato = tracciatore.evento(ev)
      // quale descrittore è l'attuale lo chiediamo a tor a ogni id nuovo (avvio, cambio di periodo)
      if (tracciatore.idVisti().length > idNoti) { idNoti = tracciatore.idVisti().length; chiediAttuale() }
      if (pubblicato) fatto()
    })
    chiediAttuale = () => {
      if (chiedi) clearTimeout(chiedi)
      chiedi = setTimeout(async () => {
        chiedi = null
        const r = await ctl.comando(`GETINFO hs/service/desc/id/${onion.replace(/\.onion$/, '')}`)
        const id = r.codice === 250 ? chiaveAccecataDalDescrittore(r.righe.join('\n')) : null
        if (!id) { o.log('tor: descrittore attuale non leggibile'); return }
        o.log('tor: descrittore attuale individuato')
        segnaAttualeNoto()
        if (tracciatore.impostaAttuale(id)) fatto()
      }, 1500)
    }
    const auth = await ctl.comando(`AUTHENTICATE ${cookie}`)
    if (auth.codice !== 250) throw new Error(`tor: porta di controllo, autenticazione rifiutata (${auth.codice})`)
    await ctl.comando('SETEVENTS HS_DESC')
    const r = await ctl.comando(`ADD_ONION ${chiave || 'NEW:ED25519-V3'} Port=80,127.0.0.1:${o.ingressPort}${chiave ? ' Flags=DiscardPK' : ''}`)
    if (r.codice !== 250) throw new Error(`tor: creazione dell'onion rifiutata (${r.codice})`)
    const id = r.righe.map(x => /^ServiceID=([a-z2-7]{56})$/.exec(x)?.[1]).find(Boolean)
    if (!id) throw new Error('tor: ADD_ONION senza ServiceID')
    if (onion && `${id}.onion` !== onion) throw new Error("tor: dopo la riconnessione l'onion è diverso (chiave non conservata)")
    onion = `${id}.onion`
    if (!chiave) {
      const nuova = r.righe.map(x => /^PrivateKey=(\S+)$/.exec(x)?.[1]).find(Boolean)
      if (!nuova) throw new Error('tor: ADD_ONION NEW senza chiave da conservare')
      chiave = nuova
      o.salvaChiaveOnion?.(nuova)   // sigillata nel vault del nodo (K6), mai su disco in chiaro
    }
    // connessione persa con il nodo acceso: l'onion è sparito, ci si ricollega
    sock.once('close', () => { if (!fermato) ricollega(1) })
  }

  /** @param {number} tentativo */
  function ricollega(tentativo) {
    if (fermato) return
    const attesa = Math.min(30, 2 ** (tentativo - 1))
    o.log(`tor: connessione di controllo persa, ricollego tra ${attesa} s (tentativo ${tentativo})`)
    riprova = setTimeout(async () => {
      riprova = null
      if (fermato) return
      try {
        await collega()
        o.log('tor: onion ricreato dopo la riconnessione (stesso indirizzo)')
      } catch (e) {
        try { sock?.destroy() } catch {}
        o.log(`tor: riconnessione fallita (${/** @type {Error} */ (e).message.slice(0, 80)})`)
        ricollega(tentativo + 1)
      }
    }, attesa * 1000)
  }

  try { await collega() } catch (e) { stop(); throw e }
  o.log('tor: pronto, servizio onion attivo')
  return {
    socksPort, onion, stop, published, statoPubblicazione: () => tracciatore.stato(), attualeNoto, controlPort, cookieFile,
    /** Solo per i test (I-59): fa cadere la connessione di controllo come farebbe un guasto. */
    chiudiControlloPerProva: () => { try { sock?.destroy() } catch {} },
    pid: child.pid,
  }
}

module.exports = { startTor, freePort, buildTorrc, hsDescLog, creaTracciatore, attendiAnnuncio, chiaveAccecataDalDescrittore, creaControllo, PUBBLICAZIONE_MINIMA }
