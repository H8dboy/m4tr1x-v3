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
  chi.textContent = `${String(p.autore).slice(0, 12)}… · ${quando}`
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

document.addEventListener('DOMContentLoaded', () => {
  document.getElementById('aggiorna-feed')?.addEventListener('click', aggiornaFeed)
  document.getElementById('crea')?.addEventListener('click', () => apriSeme('crea'))
  document.getElementById('recupera')?.addEventListener('click', () => apriSeme('recupera'))
  aggiornaStato()
  aggiornaIdentita()
})
