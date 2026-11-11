// @ts-check
'use strict'
// Interfaccia di M4TR1X. Regole (I-31, I-01, I-58):
// - nessuna chiave qui: firme e segreti restano nel lato Rust; qui arrivano solo
//   chiavi PUBBLICHE; le 24 parole compaiono solo nella finestra del seme;
// - nessuna rete qui (elenco di ciò che è vietato: inv-36-transport-only.test.js);
//   tutto passa da invoke() verso i comandi elencati in app/shell/build.rs;
// - testo inserito solo con textContent (mai HTML costruito con dati): nomi, post e
//   messaggi d'errore sono scritti da altri e non devono mai diventare markup.

/** @type {(cmd: string, args?: object) => Promise<any>} */
const invoke = /** @type {any} */ (window).__TAURI__.core.invoke

const NOMI = { spento: 'spento', avvio: 'in avvio', pronto: 'pronto', errore: 'errore' }
const SVG = 'http://www.w3.org/2000/svg'

function mostra(/** @type {string} */ id, /** @type {string} */ testo) {
  const el = document.getElementById(id)
  if (el) el.textContent = testo
}

/** Icona dallo sprite di index.html (disegno fisso: l'id è una costante del codice, mai un dato). */
function icona(/** @type {string} */ nome) {
  const svg = document.createElementNS(SVG, 'svg')
  svg.setAttribute('class', 'ic')
  const use = document.createElementNS(SVG, 'use')
  use.setAttribute('href', `#i-${nome}`)
  svg.append(use)
  return svg
}

// ─── Sezioni ──────────────────────────────────────────────────────────────────

const VISTE = ['signal', 'messaggi', 'profilo', 'rete']

function apriVista(/** @type {string} */ nome) {
  if (!VISTE.includes(nome)) return
  for (const v of VISTE) {
    const el = document.getElementById(`vista-${v}`)
    if (el) el.hidden = v !== nome
  }
  for (const b of document.querySelectorAll('.voce')) {
    const attiva = /** @type {HTMLElement} */ (b).dataset.vista === nome
    b.classList.toggle('attiva', attiva)
    if (attiva) b.setAttribute('aria-current', 'page'); else b.removeAttribute('aria-current')
  }
  if (nome === 'profilo' && identita && !profiloChiesto) caricaProfilo()
  if (nome === 'messaggi' && identita && !postaChiesta) caricaPosta()
}

// ─── Stato di Tor ─────────────────────────────────────────────────────────────

/** "spento" all'apertura può voler dire "tor non ancora partito": si riprova per qualche secondo. */
let tentativiSpento = 5
let feedPartito = false

/** Stato del trasporto: si richiede ogni secondo finché tor è in avvio (poi si ferma). */
async function aggiornaStato() {
  let ancora = false
  try {
    const s = await invoke('stato')
    const nome = NOMI[/** @type {keyof typeof NOMI} */ (s.trasporto)] || '?'
    mostra('versione', String(s.versione))
    mostra('trasporto', nome)
    mostra('rete-tor', nome)
    mostra('percento', s.trasporto === 'avvio' ? `${Number(s.percento)}%` : '')
    const spia = document.getElementById('spia')
    if (spia) spia.className = `spia ${Object.hasOwn(NOMI, s.trasporto) ? s.trasporto : ''}`
    ancora = s.trasporto === 'avvio' || (s.trasporto === 'spento' && tentativiSpento-- > 0)
    // appena Tor è pronto il feed parte da solo, una volta: poi decide chi usa l'app
    if (s.trasporto === 'pronto' && !feedPartito) { feedPartito = true; aggiornaFeed() }
  } catch {
    mostra('trasporto', 'non raggiungibile')
    mostra('rete-tor', 'non raggiungibile')
  }
  if (ancora) setTimeout(aggiornaStato, 1000)
}

// ─── Identità ─────────────────────────────────────────────────────────────────

/** @type {{identity: string}|null} */ let identita = null
/** @type {ReturnType<typeof setInterval>|null} */ let attesa = null

/** Tinta dell'avatar dalla chiave pubblica: una di 8 classi fisse (niente stile costruito con dati). */
function tinta(/** @type {string} */ chiave) {
  const n = parseInt(String(chiave).slice(0, 2), 16)
  return `av-${Number.isFinite(n) ? n % 8 : 7}`
}

/** @param {HTMLElement} el @param {string} chiave @param {string|null|undefined} nome */
function vestiAvatar(el, chiave, nome) {
  const iniziale = nome ? [...String(nome).trim()][0] : ''
  el.textContent = iniziale || String(chiave).slice(0, 2)
  el.classList.add(tinta(chiave))
}

async function aggiornaIdentita() {
  const id = await invoke('identita').catch(() => null)
  identita = id ? { identity: String(id.identity) } : null
  for (const el of document.querySelectorAll('.solo-con-identita')) /** @type {HTMLElement} */ (el).hidden = !identita
  for (const el of document.querySelectorAll('.solo-senza-identita')) /** @type {HTMLElement} */ (el).hidden = !!identita
  if (identita) {
    mostra('chiave-identita', identita.identity)
    const av = document.getElementById('mio-avatar')
    if (av) vestiAvatar(av, identita.identity, null)
    if (attesa) { clearInterval(attesa); attesa = null }
  }
}

async function apriSeme(/** @type {string} */ modo) {
  if (modo !== 'crea' && modo !== 'recupera') return
  await invoke('apri_finestra_seme', { modo })
  // la finestra del seme salva l'identità e si chiude: qui si controlla ogni 2 s
  if (!attesa) attesa = setInterval(aggiornaIdentita, 2000)
}

/** Copia la chiave PUBBLICA (è fatta per essere data agli altri). Mai segreti: qui non ce ne sono. */
async function copiaChiave() {
  if (!identita) return
  try {
    await navigator.clipboard.writeText(identita.identity)
    mostra('copia-chiave-testo', 'Copiata')
  } catch {
    mostra('copia-chiave-testo', 'Non riuscito')
  }
  setTimeout(() => mostra('copia-chiave-testo', 'Copia'), 2000)
}

// ─── Feed ────────────────────────────────────────────────────────────────────
// I post arrivano GIÀ verificati dal lato Rust (firma, rete, dati H8). Qui si mostrano soltanto,
// sempre con textContent. Il primo collegamento a un nodo può richiedere minuti (misura del
// 20/10/2026): la fase si mostra ogni secondo, l'app non deve sembrare bloccata.

/** @param {{fase: string, secondi: number, tentativo: number}} a */
function testoFase(a) {
  const s = Number(a.secondi), n = Number(a.tentativo)
  switch (a.fase) {
    case 'tor': return `aspetto che Tor sia pronto… ${s} s`
    case 'cerco': return n > 1 ? `sto cercando il nodo… ${s} s (tentativo ${n})` : `sto cercando il nodo… ${s} s`
    case 'scarico': return `nodo trovato, scarico i post… ${s} s`
    case 'invio': return `nodo trovato, invio… ${s} s`
    case 'attesa_ripresa': return `il nodo non risponde ancora, Tor continua a cercarlo: riprovo tra ${s} s`
    default: return ''
  }
}

/** "adesso", "12 min", "3 h", poi la data. */
function quando(/** @type {number} */ creato) {
  const d = Math.floor(Date.now() / 1000) - creato
  if (d < 60) return 'adesso'
  if (d < 3600) return `${Math.floor(d / 60)} min`
  if (d < 86400) return `${Math.floor(d / 3600)} h`
  return new Date(creato * 1000).toLocaleDateString('it-IT', { day: 'numeric', month: 'short', year: 'numeric' })
}

/** @param {any} p */
function voce(p) {
  const autore = String(p.autore)
  const li = document.createElement('li')
  const av = document.createElement('div')
  av.className = 'avatar'
  av.setAttribute('aria-hidden', 'true')
  vestiAvatar(av, autore, p.nome)
  const corpo = document.createElement('div')
  const chi = document.createElement('div')
  chi.className = 'chi'
  // nome dal profilo (già pulito nel lato Rust) e sempre l'inizio della chiave: il nome si può copiare, la chiave no
  if (p.nome) {
    const nome = document.createElement('span')
    nome.className = 'nome'
    nome.textContent = String(p.nome)
    chi.append(nome)
  }
  const chiave = document.createElement('span')
  chiave.textContent = `${autore.slice(0, 12)}…`
  const firma = document.createElement('span')
  firma.className = 'verificato'
  firma.title = 'Firma verificata'
  firma.append(icona('verified'))
  const ora = document.createElement('time')
  const creato = Number(p.creato)
  ora.dateTime = new Date(creato * 1000).toISOString()
  ora.title = new Date(creato * 1000).toLocaleString('it-IT')
  ora.textContent = quando(creato)
  chi.append(chiave, firma, ora)
  const testo = document.createElement('p')
  testo.className = 'testo'
  testo.textContent = String(p.testo)
  corpo.append(chi, testo)
  li.append(av, corpo)
  return li
}

function segnaVuoto() {
  const lista = document.getElementById('feed-lista')
  const vuoto = document.getElementById('feed-vuoto')
  if (lista && vuoto) vuoto.hidden = lista.childElementCount > 0
}

let feedInCorso = false

async function aggiornaFeed() {
  if (feedInCorso) return
  feedInCorso = true
  const bottone = /** @type {HTMLButtonElement|null} */ (document.getElementById('aggiorna-feed'))
  if (bottone) bottone.disabled = true
  const fase = setInterval(async () => {
    const a = await invoke('feed_avanzamento').catch(() => null)
    if (a) mostra('feed-stato', testoFase(a))
  }, 1000)
  mostra('feed-stato', 'sto cercando il nodo…')
  try {
    const f = await invoke('feed')
    const lista = document.getElementById('feed-lista')
    if (lista) lista.replaceChildren(...f.post.map(voce))
    const scartati = Number(f.scartati) ? ` · ${Number(f.scartati)} scartati (non validi)` : ''
    mostra('feed-stato', `${f.post.length} post in ${Number(f.secondi)} s${scartati}`)
  } catch (e) {
    mostra('feed-stato', String(e))
  } finally {
    clearInterval(fase)
    if (bottone) bottone.disabled = false
    feedInCorso = false
    segnaVuoto()
  }
}

// ─── Scrivi ───────────────────────────────────────────────────────────────────
// Qui arriva solo il TESTO: la firma la fa il lato Rust con la tua identità (la chiave non passa mai di
// qui, I-31). Se la pubblicazione non riesce, il testo resta nella casella.

const MASSIMO_POST = 2000

function contaPost() {
  const t = /** @type {HTMLTextAreaElement|null} */ (document.getElementById('post-testo'))
  mostra('post-conta', `${t ? [...t.value].length : 0}/${MASSIMO_POST}`)
}

async function pubblicaPost() {
  const casella = /** @type {HTMLTextAreaElement|null} */ (document.getElementById('post-testo'))
  const bottone = /** @type {HTMLButtonElement|null} */ (document.getElementById('pubblica'))
  if (!casella || !casella.value.trim()) return
  if (bottone) bottone.disabled = true
  const fase = setInterval(async () => {
    const a = await invoke('pubblica_avanzamento').catch(() => null)
    if (a) mostra('post-stato', testoFase(a))
  }, 1000)
  mostra('post-stato', 'firmo e cerco il nodo…')
  try {
    const p = await invoke('pubblica', { testo: casella.value })
    casella.value = ''
    contaPost()
    mostra('post-stato', 'pubblicato ✓')
    if (mioNome) p.nome = mioNome
    document.getElementById('feed-lista')?.prepend(voce(p))
    segnaVuoto()
  } catch (e) {
    mostra('post-stato', String(e)) // il testo resta nella casella
  } finally {
    clearInterval(fase)
    if (bottone) bottone.disabled = false
  }
}

// ─── Profilo ──────────────────────────────────────────────────────────────────
// Solo nome e descrizione (l'immagine arriva in F6). La firma la fa il lato Rust con la tua identità.

let profiloChiesto = false
/** @type {string|null} */ let mioNome = null

/** Il nome nel riquadro in alto: quello tornato dal lato Rust (già pulito), mai quello appena digitato. */
function mostraMioNome(/** @type {string|null} */ nome) {
  mioNome = nome || null
  mostra('mio-nome', mioNome || 'Senza nome')
  const av = document.getElementById('mio-avatar')
  if (av && identita) vestiAvatar(av, identita.identity, mioNome)
}

/** @param {string} comando @param {() => Promise<void>} lavoro */
async function conFaseProfilo(comando, lavoro) {
  const bottoni = ['profilo-salva', 'profilo-carica'].map(id => /** @type {HTMLButtonElement|null} */ (document.getElementById(id)))
  bottoni.forEach(b => { if (b) b.disabled = true })
  const fase = setInterval(async () => {
    const a = await invoke('profilo_avanzamento').catch(() => null)
    if (a) mostra('profilo-stato', testoFase(a))
  }, 1000)
  mostra('profilo-stato', comando)
  try { await lavoro() } catch (e) { mostra('profilo-stato', String(e)) } finally {
    clearInterval(fase)
    bottoni.forEach(b => { if (b) b.disabled = false })
  }
}

function campo(/** @type {string} */ id) {
  return /** @type {HTMLInputElement|HTMLTextAreaElement} */ (document.getElementById(id))
}

async function caricaProfilo() {
  profiloChiesto = true
  await conFaseProfilo('cerco il tuo profilo sul nodo…', async () => {
    const p = await invoke('profilo_mio')
    if (p) { campo('profilo-nome').value = String(p.nome); campo('profilo-descrizione').value = String(p.descrizione) }
    mostraMioNome(p ? String(p.nome) : null)
    mostra('profilo-stato', p ? 'profilo caricato dal nodo' : 'non hai ancora un profilo sul nodo')
  })
}

async function salvaProfilo() {
  await conFaseProfilo('firmo e cerco il nodo…', async () => {
    const p = await invoke('profilo_salva', { nome: campo('profilo-nome').value, descrizione: campo('profilo-descrizione').value })
    // il profilo com'è stato firmato (pulito nel lato Rust): quello che vedranno gli altri
    campo('profilo-nome').value = String(p.nome)
    campo('profilo-descrizione').value = String(p.descrizione)
    mostraMioNome(String(p.nome))
    mostra('profilo-stato', 'profilo salvato ✓')
  })
}

// ─── Messaggi privati ─────────────────────────────────────────────────────────
// Le buste si aprono e si incartano nel lato Rust (m4tr1x-posta): qui arrivano testi già controllati e
// chiavi PUBBLICHE. Una conversazione senza `identita` è di un mittente SCONOSCIUTO: nessun nome, nessuna
// risposta. Tutto con textContent: il testo di un messaggio lo scrive un altro.

/** @typedef {{mio: boolean, testo: string, creato: number}} Messaggio */
/** @typedef {{identita: string|null, dm: string, nome: string|null, messaggi: Messaggio[]}} Conversazione */

let postaChiesta = false
let postaInCorso = false
/** @type {Conversazione[]} */ let conversazioni = []
/** chiave DM della conversazione aperta, oppure 'nuovo' @type {string|null} */ let aperta = null

function nascondi(/** @type {string} */ id, /** @type {boolean} */ si) {
  const el = document.getElementById(id)
  if (el) el.hidden = si
}

/** @param {Conversazione} c */
function titoloDi(c) {
  if (!c.identita) return 'Mittente sconosciuto'
  return c.nome ? String(c.nome) : `${String(c.identita).slice(0, 12)}…`
}

/** @param {Messaggio} m */
function bolla(m) {
  const li = document.createElement('li')
  if (m.mio) li.className = 'mio'
  const testo = document.createElement('p')
  testo.className = 'testo'
  testo.textContent = String(m.testo)
  const ora = document.createElement('time')
  const creato = Number(m.creato)
  ora.dateTime = new Date(creato * 1000).toISOString()
  ora.textContent = new Date(creato * 1000).toLocaleString('it-IT', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })
  li.append(testo, ora)
  return li
}

function disegnaPosta() {
  const lista = document.getElementById('posta-lista')
  if (lista) {
    lista.replaceChildren(...conversazioni.map(c => {
      const li = document.createElement('li')
      const b = document.createElement('button')
      b.type = 'button'
      if (c.dm === aperta) b.className = 'scelta'
      const av = document.createElement('div')
      av.className = 'avatar'
      av.setAttribute('aria-hidden', 'true')
      vestiAvatar(av, c.identita || c.dm, c.identita ? c.nome : '?')
      const testi = document.createElement('div')
      const chi = document.createElement('span')
      chi.className = c.identita ? 'chi-conv' : 'chi-conv ignoto'
      chi.textContent = titoloDi(c)
      const ultimo = document.createElement('span')
      ultimo.className = 'ultimo'
      const m = c.messaggi[c.messaggi.length - 1]
      ultimo.textContent = m ? `${m.mio ? 'Tu: ' : ''}${String(m.testo).slice(0, 80)}` : ''
      testi.append(chi, ultimo)
      b.append(av, testi)
      b.addEventListener('click', () => { aperta = c.dm; disegnaPosta() })
      li.append(b)
      return li
    }))
  }
  const c = conversazioni.find(x => x.dm === aperta) || null
  const nuovo = aperta === 'nuovo'
  nascondi('filo-vuoto', !!c || nuovo)
  nascondi('filo-nuovo', !nuovo)
  nascondi('filo-aperto', !c)
  nascondi('filo-scrivi', !(nuovo || (c && c.identita)))
  if (!c) return
  mostra('filo-nome', titoloDi(c))
  mostra('filo-chiave', c.identita ? String(c.identita) : '')
  nascondi('filo-sconosciuto', !!c.identita)
  const av = document.getElementById('filo-avatar')
  if (av) { av.className = 'avatar'; vestiAvatar(av, c.identita || c.dm, c.identita ? c.nome : '?') }
  document.getElementById('filo-messaggi')?.replaceChildren(...c.messaggi.map(bolla))
}

/** @param {string} comando @param {() => Promise<void>} lavoro */
async function conFasePosta(comando, lavoro) {
  if (postaInCorso) return
  postaInCorso = true
  const bottoni = ['posta-aggiorna', 'filo-invia'].map(id => /** @type {HTMLButtonElement|null} */ (document.getElementById(id)))
  bottoni.forEach(b => { if (b) b.disabled = true })
  const fase = setInterval(async () => {
    const a = await invoke('messaggi_avanzamento').catch(() => null)
    if (a) mostra('posta-stato', testoFase(a))
  }, 1000)
  mostra('posta-stato', comando)
  try { await lavoro() } catch (e) { mostra('posta-stato', String(e)) } finally {
    clearInterval(fase)
    bottoni.forEach(b => { if (b) b.disabled = false })
    postaInCorso = false
  }
}

async function caricaPosta() {
  postaChiesta = true
  await conFasePosta('cerco i tuoi messaggi sul nodo…', async () => {
    const p = await invoke('messaggi')
    conversazioni = Array.isArray(p.conversazioni) ? p.conversazioni : []
    const scartate = Number(p.scartate) ? ` · ${Number(p.scartate)} buste scartate (non valide o non per te)` : ''
    mostra('posta-stato', conversazioni.length ? `${conversazioni.length} conversazioni${scartate}` : `nessun messaggio${scartate}`)
    disegnaPosta()
  })
}

async function inviaMessaggio() {
  const casella = /** @type {HTMLTextAreaElement} */ (document.getElementById('filo-testo'))
  const c = conversazioni.find(x => x.dm === aperta) || null
  const a = aperta === 'nuovo' ? campo('nuovo-a').value.trim().toLowerCase() : (c && c.identita) || ''
  if (!a || !casella.value.trim()) return
  let partito = false
  await conFasePosta('incarto e cerco il nodo…', async () => {
    await invoke('messaggio_invia', { a, testo: casella.value })
    casella.value = ''
    partito = true
    mostra('posta-stato', 'inviato ✓')
  })
  if (!partito) return // il testo resta nella casella
  // si rilegge dal nodo: il messaggio compare dalla MIA copia cifrata, non da quello che ho digitato
  await caricaPosta()
  const mia = conversazioni.find(x => x.identita === a)
  if (mia) { aperta = mia.dm; campo('nuovo-a').value = ''; disegnaPosta() }
}

document.addEventListener('DOMContentLoaded', () => {
  document.getElementById('posta-aggiorna')?.addEventListener('click', caricaPosta)
  document.getElementById('posta-nuovo')?.addEventListener('click', () => { aperta = 'nuovo'; disegnaPosta() })
  document.getElementById('filo-invia')?.addEventListener('click', inviaMessaggio)
  for (const b of document.querySelectorAll('.voce')) b.addEventListener('click', () => apriVista(/** @type {HTMLElement} */ (b).dataset.vista || ''))
  for (const b of document.querySelectorAll('[data-seme]')) b.addEventListener('click', () => apriSeme(/** @type {HTMLElement} */ (b).dataset.seme || ''))
  document.getElementById('copia-chiave')?.addEventListener('click', copiaChiave)
  document.getElementById('profilo-salva')?.addEventListener('click', salvaProfilo)
  document.getElementById('profilo-carica')?.addEventListener('click', caricaProfilo)
  document.getElementById('pubblica')?.addEventListener('click', pubblicaPost)
  document.getElementById('post-testo')?.addEventListener('input', contaPost)
  document.getElementById('aggiorna-feed')?.addEventListener('click', aggiornaFeed)
  mostraMioNome(null)
  aggiornaStato()
  aggiornaIdentita()
})
