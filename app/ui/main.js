// @ts-check
'use strict'
// Interfaccia di M4TR1X. Regole (I-31, I-01, I-58):
// - nessuna chiave qui: firme e segreti restano nel lato Rust; qui arrivano solo
//   chiavi PUBBLICHE; le 24 parole compaiono solo nella finestra del seme;
// - nessuna rete qui (elenco di ciò che è vietato: inv-36-transport-only.test.js);
//   tutto passa da invoke() verso i comandi elencati in app/shell/build.rs;
// - testo inserito solo con textContent (mai innerHTML con dati).

/** @type {(cmd: string, args?: object) => Promise<any>} */
const invoke = /** @type {any} */ (window).__TAURI__.core.invoke

const NOMI = { spento: 'spento', avvio: 'in avvio', pronto: 'pronto', errore: 'errore' }

function mostra(/** @type {string} */ id, /** @type {string} */ testo) {
  const el = document.getElementById(id)
  if (el) el.textContent = testo
}

function vedi(/** @type {string} */ id, /** @type {boolean} */ si) {
  const el = document.getElementById(id)
  if (el) el.hidden = !si
}

/** "spento" all'apertura può voler dire "tor non ancora partito": si riprova per qualche secondo. */
let tentativiSpento = 5

/** Stato del trasporto: si richiede ogni secondo finché tor è in avvio (poi si ferma). */
async function aggiornaStato() {
  let ancora = false
  try {
    const s = await invoke('stato')
    mostra('versione', String(s.versione))
    mostra('trasporto', NOMI[/** @type {keyof typeof NOMI} */ (s.trasporto)] || '?')
    mostra('percento', s.trasporto === 'avvio' ? `${Number(s.percento)}%` : '')
    ancora = s.trasporto === 'avvio' || (s.trasporto === 'spento' && tentativiSpento-- > 0)
  } catch {
    mostra('trasporto', 'non raggiungibile')
  }
  if (ancora) setTimeout(aggiornaStato, 1000)
}

/** @type {ReturnType<typeof setInterval>|null} */ let attesa = null

async function aggiornaIdentita() {
  const id = await invoke('identita').catch(() => null)
  vedi('senza-identita', !id)
  vedi('con-identita', !!id)
  if (id) {
    mostra('chiave-identita', String(id.identity))
    if (attesa) { clearInterval(attesa); attesa = null }
  }
}

async function apriSeme(/** @type {'crea'|'recupera'} */ modo) {
  await invoke('apri_finestra_seme', { modo })
  // la finestra del seme salva l'identità e si chiude: qui si controlla ogni 2 s
  if (!attesa) attesa = setInterval(aggiornaIdentita, 2000)
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
    case 'invio': return `nodo trovato, invio il post firmato… ${s} s`
    case 'attesa_ripresa': return `il nodo non risponde ancora, Tor continua a cercarlo: riprovo tra ${s} s`
    default: return ''
  }
}

/** @param {any} p */
function voce(p) {
  const li = document.createElement('li')
  const chi = document.createElement('div')
  chi.className = 'chi'
  const quando = new Date(Number(p.creato) * 1000).toLocaleString('it-IT')
  // nome dal profilo (già pulito nel lato Rust) e sempre l'inizio della chiave: il nome si può copiare, la chiave no
  if (p.nome) {
    const nome = document.createElement('span')
    nome.className = 'nome'
    nome.textContent = String(p.nome)
    chi.append(nome, ' · ')
  }
  chi.append(`${String(p.autore).slice(0, 12)}… · ${quando}`)
  const testo = document.createElement('p')
  testo.className = 'testo'
  testo.textContent = String(p.testo)
  li.append(chi, testo)
  return li
}

async function aggiornaFeed() {
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
    document.getElementById('feed-lista')?.prepend(voce(p))
  } catch (e) {
    mostra('post-stato', String(e)) // il testo resta nella casella
  } finally {
    clearInterval(fase)
    if (bottone) bottone.disabled = false
  }
}

// ─── Profilo ──────────────────────────────────────────────────────────────────
// Solo nome e descrizione (l'immagine arriva in F6). La firma la fa il lato Rust con la tua identità.

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
  await conFaseProfilo('cerco il tuo profilo sul nodo…', async () => {
    const p = await invoke('profilo_mio')
    if (p) { campo('profilo-nome').value = String(p.nome); campo('profilo-descrizione').value = String(p.descrizione) }
    mostra('profilo-stato', p ? 'profilo caricato dal nodo' : 'non hai ancora un profilo sul nodo')
  })
}

async function salvaProfilo() {
  await conFaseProfilo('firmo e cerco il nodo…', async () => {
    const p = await invoke('profilo_salva', { nome: campo('profilo-nome').value, descrizione: campo('profilo-descrizione').value })
    // il profilo com'è stato firmato (pulito nel lato Rust): quello che vedranno gli altri
    campo('profilo-nome').value = String(p.nome)
    campo('profilo-descrizione').value = String(p.descrizione)
    mostra('profilo-stato', 'profilo salvato ✓')
  })
}

document.addEventListener('DOMContentLoaded', () => {
  document.getElementById('profilo-salva')?.addEventListener('click', salvaProfilo)
  document.getElementById('profilo-carica')?.addEventListener('click', caricaProfilo)
  document.getElementById('pubblica')?.addEventListener('click', pubblicaPost)
  document.getElementById('post-testo')?.addEventListener('input', contaPost)
  document.getElementById('aggiorna-feed')?.addEventListener('click', aggiornaFeed)
  document.getElementById('crea')?.addEventListener('click', () => apriSeme('crea'))
  document.getElementById('recupera')?.addEventListener('click', () => apriSeme('recupera'))
  aggiornaStato()
  aggiornaIdentita()
})
