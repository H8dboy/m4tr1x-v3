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

const VISTE = ['signal', 'media', 'forum', 'messaggi', 'profilo', 'rete']
/** le tre voci Reels, Film, Musica aprono la stessa sezione con la sezione di M4TR1X giusta */
const SEZIONI = { reels: { nome: 'signal', titolo: 'Reels', file: 'Foto o video (JPEG, PNG, MP4, MOV)', accetta: 'image/jpeg,image/png,video/mp4,video/quicktime', icona: 'reel' }, film: { nome: 'film', titolo: 'Film', file: 'Video (MP4, MOV)', accetta: 'video/mp4,video/quicktime', icona: 'film' }, musica: { nome: 'musica', titolo: 'Musica', file: 'Brano (MP3, M4A)', accetta: 'audio/mpeg,audio/mp4,.m4a,.mp3', icona: 'music' } }

function apriVista(/** @type {string} */ voce) {
  const sez = Object.hasOwn(SEZIONI, voce) ? SEZIONI[/** @type {keyof typeof SEZIONI} */ (voce)] : null
  const nome = sez ? 'media' : voce
  if (!VISTE.includes(nome)) return
  for (const v of VISTE) {
    const el = document.getElementById(`vista-${v}`)
    if (el) el.hidden = v !== nome
  }
  for (const b of document.querySelectorAll('.voce')) {
    const attiva = /** @type {HTMLElement} */ (b).dataset.vista === voce
    b.classList.toggle('attiva', attiva)
    if (attiva) b.setAttribute('aria-current', 'page'); else b.removeAttribute('aria-current')
  }
  if (nome === 'profilo' && identita && !profiloChiesto) caricaProfilo()
  if (nome === 'messaggi' && identita && !postaChiesta) caricaPosta()
  if (nome === 'forum' && !forumChiesto) caricaForum()
  if (sez) apriSezione(sez)
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
    if (s.trasporto === 'pronto' && !feedPartito && await aggiornaNodo()) { feedPartito = true; aggiornaFeed() }
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

// ─── Reels, film, musica ──────────────────────────────────────────────────────
// Le schede arrivano GIÀ verificate dal lato Rust. Il file si scarica solo quando lo apri: torna come byte già
// controllati (hash e tipo) e si mostra con un indirizzo blob: del tipo della scheda (elenco chiuso nel lato Rust).
// Per pubblicare, i byte del file scelto vanno al lato Rust, che lo PULISCE prima di tutto il resto.

/** @typedef {{sha256: string, mime: string, dimensione: number, sezione: string, titolo: string}} FileMedia */
/** @typedef {{id: string, autore: string, nome: string|null, testo: string, creato: number, file: FileMedia}} Media */

/** @type {typeof SEZIONI[keyof typeof SEZIONI]} */ let sezioneAperta = SEZIONI.reels
let mediaInCorso = false
/** @type {string|null} */ let urlAperto = null

function mega(/** @type {number} */ n) {
  return n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`
}

/** @param {Media} m */
function voceMedia(m) {
  const li = document.createElement('li')
  const b = document.createElement('button')
  b.type = 'button'
  const cop = document.createElement('span')
  cop.className = 'copertina'
  const tipo = String(m.file.mime)
  cop.append(icona(tipo.startsWith('audio/') ? 'music' : tipo.startsWith('image/') ? 'photo' : 'film'))
  const t = document.createElement('span')
  t.className = 't'
  t.textContent = String(m.file.titolo) || String(m.testo).slice(0, 60) || 'Senza titolo'
  const meta = document.createElement('span')
  meta.className = 'm'
  meta.textContent = `${m.nome ? String(m.nome) : String(m.autore).slice(0, 12) + '…'} · ${quando(Number(m.creato))} · ${mega(Number(m.file.dimensione))}`
  b.append(cop, t, meta)
  b.addEventListener('click', () => apriMedia(m))
  li.append(b)
  return li
}

/** @param {string} comando @param {() => Promise<void>} lavoro */
async function conFaseMedia(comando, lavoro) {
  if (mediaInCorso) return
  mediaInCorso = true
  const bottoni = ['media-aggiorna', 'media-pubblica'].map(id => /** @type {HTMLButtonElement|null} */ (document.getElementById(id)))
  bottoni.forEach(b => { if (b) b.disabled = true })
  const fase = setInterval(async () => {
    const a = await invoke('media_avanzamento').catch(() => null)
    if (a) mostra('media-stato', testoFase(a))
  }, 1000)
  mostra('media-stato', comando)
  try { await lavoro() } catch (e) { mostra('media-stato', String(e)) } finally {
    clearInterval(fase)
    const pub = /** @type {HTMLButtonElement|null} */ (document.getElementById('media-pubblica'))
    bottoni.forEach(b => { if (b && b !== pub) b.disabled = false })
    if (pub) pub.disabled = !filePronto
    mediaInCorso = false
  }
}

function chiudiVisore() {
  if (urlAperto) { URL.revokeObjectURL(urlAperto); urlAperto = null }
  document.getElementById('media-schermo')?.replaceChildren()
  nascondi('media-visore', true)
}

/** @param {typeof SEZIONI[keyof typeof SEZIONI]} s */
function apriSezione(s) {
  if (sezioneAperta !== s) { chiudiVisore(); document.getElementById('media-lista')?.replaceChildren() }
  sezioneAperta = s
  mostra('t-media', s.titolo)
  mostra('media-cosa', s.file)
  const scegli = /** @type {HTMLInputElement|null} */ (document.getElementById('media-scegli'))
  if (scegli) scegli.accept = s.accetta
  caricaMedia()
}

async function caricaMedia() {
  const s = sezioneAperta
  await conFaseMedia('cerco i contenuti sul nodo…', async () => {
    const g = await invoke('media', { sezione: s.nome })
    if (s !== sezioneAperta) return
    const lista = Array.isArray(g.media) ? g.media : []
    document.getElementById('media-lista')?.replaceChildren(...lista.map(voceMedia))
    nascondi('media-vuoto', lista.length > 0)
    const scartati = Number(g.scartati) ? ` · ${Number(g.scartati)} scartati (non validi)` : ''
    mostra('media-stato', `${lista.length} contenuti${scartati}`)
  })
}

/** @param {Media} m */
async function apriMedia(m) {
  await conFaseMedia(`scarico ${mega(Number(m.file.dimensione))} via Tor…`, async () => {
    /** @type {ArrayBuffer} */
    const byte = await invoke('media_file', { sha256: m.file.sha256, mime: m.file.mime, dimensione: m.file.dimensione })
    chiudiVisore()
    const tipo = String(m.file.mime)
    urlAperto = URL.createObjectURL(new Blob([byte], { type: tipo }))
    /** @type {HTMLElement} */ let el
    if (tipo.startsWith('image/')) { const i = document.createElement('img'); i.src = urlAperto; i.alt = String(m.file.titolo); el = i }
    else if (tipo.startsWith('audio/')) { const a = document.createElement('audio'); a.src = urlAperto; a.controls = true; el = a }
    else { const v = document.createElement('video'); v.src = urlAperto; v.controls = true; v.playsInline = true; el = v }
    document.getElementById('media-schermo')?.append(el)
    mostra('media-v-titolo', String(m.file.titolo) || 'Senza titolo')
    mostra('media-v-chi', `${m.nome ? String(m.nome) : String(m.autore).slice(0, 12) + '…'} · ${quando(Number(m.creato))}`)
    mostra('media-v-testo', String(m.testo))
    nascondi('media-visore', false)
    mostra('media-stato', 'file verificato ✓')
    document.getElementById('media-visore')?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  })
}

let filePronto = false

async function sceltoFile() {
  const scegli = /** @type {HTMLInputElement} */ (document.getElementById('media-scegli'))
  const f = scegli.files && scegli.files[0]
  filePronto = false
  const pub = /** @type {HTMLButtonElement|null} */ (document.getElementById('media-pubblica'))
  if (pub) pub.disabled = true
  mostra('media-pulito', '')
  if (!f) return
  if (f.size > 50 * 1024 * 1024) { mostra('media-pulito', 'file troppo grande (massimo 50 MB)'); return }
  mostra('media-pulito', 'pulisco il file sul tuo computer…')
  try {
    // solo il CONTENUTO: il nome del file e la cartella non vanno al lato Rust né al nodo
    const p = await invoke('media_prepara', new Uint8Array(await f.arrayBuffer()))
    filePronto = true
    if (pub) pub.disabled = false
    mostra('media-pulito', `pulito ✓ · ${String(p.mime)} · ${mega(Number(p.prima))} → ${mega(Number(p.dimensione))}`)
  } catch (e) {
    mostra('media-pulito', String(e))
  }
}

async function pubblicaMedia() {
  if (!filePronto) return
  const s = sezioneAperta
  let fatto = false
  await conFaseMedia('firmo, carico il file e cerco il nodo…', async () => {
    await invoke('media_pubblica', { sezione: s.nome, titolo: campo('media-titolo').value, testo: campo('media-testo').value })
    fatto = true
    filePronto = false
    campo('media-titolo').value = ''; campo('media-testo').value = ''
    const scegli = /** @type {HTMLInputElement} */ (document.getElementById('media-scegli')); scegli.value = ''
    mostra('media-pulito', '')
    nascondi('media-scrivi', true)
  })
  if (fatto) await caricaMedia()
}

// ─── Forum ────────────────────────────────────────────────────────────────────
// Stesso schema della v2. Discussioni e risposte arrivano GIÀ verificate dal lato Rust; qui solo textContent.
// Si scrive firmando con la tua identità nel lato Rust (come i post).

/** @typedef {{id: string, autore: string, nome: string|null, titolo: string, testo: string, creato: number, risposte: number, attivita: number}} Discussione */

let forumChiesto = false
let forumInCorso = false
/** id della discussione aperta @type {string|null} */ let discAperta = null

/** @param {Discussione} d */
function chiDi(d) {
  return d.nome ? String(d.nome) : `${String(d.autore).slice(0, 12)}…`
}

/** @param {Discussione} d */
function voceDiscussione(d) {
  const li = document.createElement('li')
  const b = document.createElement('button')
  b.type = 'button'
  const t = document.createElement('span')
  t.className = 'titolo-disc'
  t.textContent = String(d.titolo) || 'Senza titolo'
  const a = document.createElement('span')
  a.className = 'anteprima'
  a.textContent = String(d.testo).slice(0, 160)
  const m = document.createElement('span')
  m.className = 'meta'
  const n = document.createElement('span')
  n.className = 'n'
  n.textContent = Number(d.risposte) === 1 ? '1 risposta' : `${Number(d.risposte)} risposte`
  m.append(`${chiDi(d)} · ${quando(Number(d.attivita))} · `, n)
  b.append(t, a, m)
  b.addEventListener('click', () => apriDiscussione(String(d.id)))
  li.append(b)
  return li
}

/** @param {string} comando @param {() => Promise<void>} lavoro */
async function conFaseForum(comando, lavoro) {
  if (forumInCorso) return
  forumInCorso = true
  const bottoni = ['forum-aggiorna', 'forum-pubblica', 'disc-rispondi'].map(id => /** @type {HTMLButtonElement|null} */ (document.getElementById(id)))
  bottoni.forEach(b => { if (b) b.disabled = true })
  const fase = setInterval(async () => {
    const a = await invoke('forum_avanzamento').catch(() => null)
    if (a) mostra('forum-stato', testoFase(a))
  }, 1000)
  mostra('forum-stato', comando)
  try { await lavoro() } catch (e) { mostra('forum-stato', String(e)) } finally {
    clearInterval(fase)
    bottoni.forEach(b => { if (b) b.disabled = false })
    forumInCorso = false
  }
}

function vediElenco() {
  discAperta = null
  nascondi('forum-aperta', true)
  nascondi('forum-elenco', false)
}

async function caricaForum() {
  forumChiesto = true
  await conFaseForum('cerco le discussioni sul nodo…', async () => {
    const f = await invoke('forum')
    const lista = Array.isArray(f.discussioni) ? f.discussioni : []
    document.getElementById('forum-lista')?.replaceChildren(...lista.map(voceDiscussione))
    nascondi('forum-vuoto', lista.length > 0)
    const scartati = Number(f.scartati) ? ` · ${Number(f.scartati)} scartati (non validi)` : ''
    mostra('forum-stato', `${lista.length} discussioni${scartati}`)
    vediElenco()
  })
}

async function apriDiscussione(/** @type {string} */ id) {
  await conFaseForum('apro la discussione…', async () => {
    const d = await invoke('forum_discussione', { id })
    if (!d) { mostra('forum-stato', 'discussione non trovata sul nodo'); return }
    discAperta = String(d.discussione.id)
    mostra('disc-titolo', String(d.discussione.titolo) || 'Senza titolo')
    mostra('disc-chi', `${chiDi(d.discussione)} · ${quando(Number(d.discussione.creato))}`)
    mostra('disc-testo', String(d.discussione.testo))
    document.getElementById('disc-risposte')?.replaceChildren(...d.risposte.map(voce))
    mostra('forum-stato', d.risposte.length === 1 ? '1 risposta' : `${d.risposte.length} risposte`)
    nascondi('forum-elenco', true)
    nascondi('forum-aperta', false)
  })
}

async function pubblicaDiscussione() {
  const titolo = campo('forum-titolo'), testo = campo('forum-testo')
  if (!titolo.value.trim() || !testo.value.trim()) { mostra('forum-stato', 'servono titolo e testo'); return }
  let fatto = false
  await conFaseForum('firmo e cerco il nodo…', async () => {
    await invoke('forum_pubblica', { titolo: titolo.value, testo: testo.value })
    titolo.value = ''; testo.value = ''
    nascondi('forum-scrivi', true)
    fatto = true
  })
  if (fatto) await caricaForum() // si rilegge dal nodo: compare com'è stata firmata e verificata
}

async function rispondiDiscussione() {
  const casella = campo('disc-risposta')
  if (!discAperta || !casella.value.trim()) return
  const id = discAperta
  let fatto = false
  await conFaseForum('firmo e cerco il nodo…', async () => {
    await invoke('forum_rispondi', { id, testo: casella.value })
    casella.value = ''
    fatto = true
  })
  if (fatto) await apriDiscussione(id)
}

// ─── Nodo ─────────────────────────────────────────────────────────────────────
// L'indirizzo si scrive qui e si controlla nel lato Rust (solo onion v3). Non torna indietro:
// l'interfaccia sa solo se un nodo c'è.

async function aggiornaNodo() {
  const si = await invoke('nodo_impostato').catch(() => false)
  mostra('nodo-attuale', si ? 'Un nodo è impostato: puoi sostituirlo scrivendone un altro.' : 'Nessun nodo: scrivi qui l\'indirizzo .onion del nodo a cui collegarti.')
  return !!si
}

async function salvaNodo() {
  const c = campo('nodo-indirizzo')
  try {
    await invoke('nodo_imposta', { indirizzo: c.value })
    c.value = ''
    mostra('nodo-stato', 'nodo impostato ✓')
    await aggiornaNodo()
    feedPartito = true
    aggiornaFeed()
  } catch (e) {
    mostra('nodo-stato', String(e))
  }
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
  document.getElementById('nodo-salva')?.addEventListener('click', salvaNodo)
  document.getElementById('media-aggiorna')?.addEventListener('click', caricaMedia)
  document.getElementById('media-nuovo')?.addEventListener('click', () => nascondi('media-scrivi', false))
  document.getElementById('media-annulla')?.addEventListener('click', () => nascondi('media-scrivi', true))
  document.getElementById('media-scegli')?.addEventListener('change', sceltoFile)
  document.getElementById('media-pubblica')?.addEventListener('click', pubblicaMedia)
  document.getElementById('forum-aggiorna')?.addEventListener('click', caricaForum)
  document.getElementById('forum-nuova')?.addEventListener('click', () => { nascondi('forum-scrivi', false); campo('forum-titolo').focus() })
  document.getElementById('forum-annulla')?.addEventListener('click', () => nascondi('forum-scrivi', true))
  document.getElementById('forum-pubblica')?.addEventListener('click', pubblicaDiscussione)
  document.getElementById('forum-indietro')?.addEventListener('click', vediElenco)
  document.getElementById('disc-rispondi')?.addEventListener('click', rispondiDiscussione)
  aggiornaNodo()
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
