// @ts-check
'use strict'
/**
 * F4, passo 4 — prova di fine corsa del feed: un nodo VERO con il suo onion, un post pubblicato, e il
 * feed dell'app (m4tr1x-feed, lato Rust) che lo legge via Tor e lo verifica.
 *
 * Misura anche il punto 46 di docs/RIPRESA.md (fondatore, 20/10/2026): tor tiene i descrittori degli
 * onion solo in memoria? Si legge il feed:
 *   1. primo collegamento               (processo 1)
 *   2. secondo nella stessa sessione    (processo 1)
 *   3. dopo un "riavvio dell'app"        (processo 2, STESSA cartella dati di tor)
 * I tempi si stampano sempre (# feed: …). La prova pretende solo che il post arrivi in ogni lettura:
 * i tempi sono una misura, non una regola.
 * Parte solo con M4TR1X_TOR_BIN e cargo (in CI: job "tor").
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const os = require('os')
const path = require('path')
const { spawn, spawnSync } = require('child_process')
const protocol = require('../../packages/protocol')
const { NET, startTestNode, wsOpen, wsExchange, testIdentity } = require('../helpers/node')

const ROOT = path.resolve(__dirname, '..', '..')
const TOR = process.env.M4TR1X_TOR_BIN
const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })
const salta = !TOR || !fs.existsSync(TOR) ? 'M4TR1X_TOR_BIN non impostato: prova del feed via Tor saltata'
  : cargo.status !== 0 ? 'cargo non disponibile' : false

/**
 * Un processo "app": tor nella cartella `dati`, due letture del feed. Righe JSON.
 * ASINCRONO, mai spawnSync: il nodo gira in QUESTO processo, e spawnSync ferma il suo ciclo degli
 * eventi — tor consegna la connessione ma il nodo non risponde (20/10/2026: "tempo massimo superato
 * nell'apertura" a ogni tentativo, colpa della prova e non dell'app).
 */
function app(/** @type {string} */ onion, /** @type {string} */ dati, esempio = 'misura', /** @type {string[]} */ altri = []) {
  return new Promise((resolve, reject) => {
    const p = spawn('cargo', ['run', '--quiet', '--locked', '--manifest-path', path.join(ROOT, 'app', 'Cargo.toml'), '-p', 'm4tr1x-feed', '--example', esempio, '--', onion, dati, ...altri],
      // senza shell: con cmd.exe un argomento con spazi (il testo del post) si spezza (20/10/2026: il nodo riceveva "Nderë:")
      { cwd: path.join(ROOT, 'app'), env: process.env, shell: false })
    let out = '', err = ''
    p.stdout.on('data', d => { out += d })
    p.stderr.on('data', d => { err += d })
    const limite = setTimeout(() => p.kill(), 1500000)
    p.on('error', reject)
    p.on('close', code => {
      clearTimeout(limite)
      if (code !== 0) return reject(new Error(`app uscita con ${code}: ${(out + err).slice(-3000)}`))
      resolve(out.split('\n').filter(l => l.startsWith('{')).map(l => JSON.parse(l)))
    })
  })
}

test('il feed dell\'app legge via Tor i post verificati di un nodo vero (e misura le riaperture)', { skip: salta, timeout: 2400000 }, async (t) => {
  const owner = testIdentity()
  const a = await startTestNode({ uploaders: [owner.publicKey], tor: true, torBin: TOR })
  const dati = fs.mkdtempSync(path.join(os.tmpdir(), 'm4-feed-app-'))
  try {
    assert.ok(a.node.onion, 'il nodo non ha un onion')
    // Fase "pubblicazione" (rete Tor, rilanciabile): si legge solo con la pubblicazione CONFERMATA. Con la sola
    // riserva dei 5 minuti il descrittore può non essere sugli HSDir (20/10/2026, giro 3: 2 letture su 4 oltre il
    // budget) e la prova misurerebbe la rete, non l'app.
    const annuncio = await a.node.announceDecision
    assert.equal(annuncio, 'confermata', 'fase "pubblicazione": il nodo è arrivato solo alla riserva dei 5 minuti (rete Tor, rilanciabile)')
    const ev = protocol.buildEvent({ kind: 1, content: 'Nderë: primo post letto dall\'app via Tor', tags: [] }, owner, NET)
    const ws = await wsOpen(a.node.port)
    const ok = await wsExchange(ws, ['EVENT', ev], m => m[0] === 'OK')
    ws.close()
    assert.equal(ok.at(-1)[2], true, JSON.stringify(ok.at(-1)))

    // il tor dell'app che non finisce il bootstrap è la rete Tor: fase "tor" (fondatore, 20/10/2026)
    const senzaTor = (/** @type {any[]} */ righe) => { const x = righe.find(r => r.errore_tor); if (x) assert.fail(`fase "tor": il tor dell'app non ha finito il bootstrap: ${x.errore_tor}`); return righe }
    const primo = senzaTor(await app(a.node.onion, dati))
    const dopo = senzaTor(await app(a.node.onion, dati)) // stesso tor, stessa cartella: "dopo un riavvio dell'app"
    const letture = [...primo.filter(x => x.lettura !== 'bootstrap'), ...dopo.filter(x => x.lettura !== 'bootstrap').map(x => ({ ...x, lettura: x.lettura === 'primo collegamento' ? 'dopo un riavvio (stessa cartella)' : 'secondo dopo il riavvio' }))]
    t.diagnostic(`feed (annuncio del nodo: ${annuncio}): ` + letture.map(x => `${x.lettura}=${x.errore ? `ERRORE ${x.errore}` : `${x.secondi}s/${x.tentativi} tent.`}`).join(' · '))
    // Fasi con le stesse parole delle altre prove con Tor (il runner rilancia una volta SOLO "pubblicazione"
    // e "rete"): onion non raggiunto → "rete"; collegati ma relay rotto → "relay"; post mancanti → "feed".
    for (const x of letture) {
      if (x.errore) assert.fail(`fase "${/relay:/.test(x.errore) ? 'relay' : 'rete'}": ${x.lettura}: ${x.errore}`)
      assert.ok(x.ids.includes(ev.id), `fase "feed": ${x.lettura}: il post pubblicato non è nel feed`)
      assert.equal(x.scartati, 0, `fase "feed": ${x.lettura}: eventi scartati da un nodo onesto`)
    }

    // F4, post: l'app FIRMA un post nel lato Rust (identità del seme pubblico di prova) e lo pubblica via Tor.
    // Il nodo lo deve avere (chiesto in locale, non via Tor) e il JavaScript lo deve verificare.
    // Fasi: onion non raggiunto → "rete"; nodo che dice no o relay rotto → "invio"; firma non valida per il
    // JavaScript → "firma"; non riletto dal feed → "feed".
    const testo = `Nderë: primo post PUBBLICATO dall'app via Tor 🦅 ${Date.now()}`
    const [pub] = senzaTor(await app(a.node.onion, dati, 'pubblica', [testo]))
    t.diagnostic(`post: ${pub.errore ? `ERRORE ${pub.errore}` : `pubblicato in ${pub.secondi}s/${pub.tentativi} tent., riletto dal feed: ${pub.riletto}, nome: ${pub.nome}`}`)
    if (pub.errore) assert.fail(`fase "${/NonRaggiungibile/.test(pub.errore) ? 'rete' : 'invio'}": pubblicazione: ${pub.errore}`)
    const ws2 = await wsOpen(a.node.port)
    const sulNodo = (await wsExchange(ws2, ['REQ', 'p', { ids: [pub.id] }], m => m[0] === 'EOSE')).find(m => m[0] === 'EVENT')?.[2]
    ws2.close()
    assert.ok(sulNodo, `fase "invio": il nodo non ha il post ${pub.id}`)
    assert.equal(sulNodo.content, testo)
    assert.equal(sulNodo.pubkey, '817d2b662e9c2c6e080c5fd5d9c4866a8342d194dbea7f98c1f4fc30e069bed7', 'fase "firma": non è l\'identità di prova')
    assert.equal(protocol.verifyEvent(sulNodo, NET), true, 'fase "firma": il JavaScript non verifica il post firmato in Rust')
    assert.equal(pub.riletto, true, 'fase "feed": il post pubblicato non si rilegge dal feed')
    // F4, profilo: l'app ha salvato (firmato, via Tor) il profilo "Prova M4TR1X" prima del post
    assert.equal(pub.nome, 'Prova M4TR1X', 'fase "feed": il post riletto non ha il nome del profilo salvato')
  } finally {
    await a.close()
    fs.rmSync(dati, { recursive: true, force: true })
  }
})
