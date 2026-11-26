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
    case 'carico_pezzo': return pezzi(a, 'carico')
    case 'scarico_pezzo': return pezzi(a, 'scarico')
    case 'codifico': return Number(a.pezzi_totali) > 1 ? `codifico a celle il fotogramma ${Number(a.pezzi_fatti) + 1} di ${Number(a.pezzi_totali)}… ${s} s` : 'codifico a celle…'
    default: return ''
  }
}

/** "carico il pezzo 3 di 12… 41 s" (un pezzo = 4 MB; i pezzi già sul nodo si saltano). */
function pezzi(/** @type {any} */ a, /** @type {string} */ verbo) {
  const fatti = Number(a.pezzi_fatti), totali = Number(a.pezzi_totali), s = Number(a.secondi)
  if (totali <= 1) return `${verbo} il file… ${s} s`
  if (fatti >= totali) return `${verbo === 'carico' ? 'caricati' : 'scaricati'} ${totali} pezzi su ${totali} ✓`
  return `${verbo} il pezzo ${fatti + 1} di ${totali}… ${s} s`
}

/** "adesso", "12 min", "3 h", poi la data. */
function quando(/** @type {number} */ creato) {
  const d = Math.floor(Date.now() / 1000) - creato
  if (d < 60) return 'adesso'
  if (d < 3600) return `${Math.floor(d / 60)} min`
  if (d < 86400) return `${Math.floor(d / 3600)} h`
  return new Date(creato * 1000).toLocaleDateString('it-IT', { day: 'numeric', month: 'short', year: 'numeric' })
}

// Bandierine della moderazione del nodo (I-65): il motivo in parole, da un elenco chiuso (categorie sconosciute:
// "segnalato dal nodo"). Il velo copre il testo; si apre con un tocco, TRANNE per il possibile sfruttamento di
// minori, che resta chiuso finché un moderatore non lo valuta (docs/design/MODERAZIONE.md).
const MOTIVI = /** @type {Record<string, string>} */ ({
  truffa: 'possibile truffa', spam: 'possibile spam', odio: "possibili contenuti d'odio", violenza: 'possibile violenza',
  armi: 'riferimenti ad armi', droga: 'riferimenti a droghe', 'sfruttamento-minori': 'possibile sfruttamento di minori',
  terrorismo: 'possibile terrorismo', altro: 'segnalato dal nodo', offuscamento: 'testo camuffato (caratteri nascosti o alfabeti mischiati)',
})
const MAI_APRIBILE = new Set(['sfruttamento-minori'])

/** Il velo su un contenuto con bandierine (post, discussione, scheda di un file); `null` se non ne ha. Gli
 * elementi `coperti` restano nascosti finché non lo apri. @param {any} p @param {HTMLElement[]} coperti */
function velo(p, ...coperti) {
  const b = Array.isArray(p.bandierine) ? p.bandierine : []
  if (!b.length) return null
  const motivi = [...new Set(b.map((/** @type {any} */ x) => MOTIVI[String(x.categoria)] || MOTIVI.altro))]
  const codice = b.some((/** @type {any} */ x) => x.codice === true)
  const v = document.createElement('div')
  v.className = 'velo'
  const perche = document.createElement('p')
  perche.textContent = `Coperto dal nodo: ${motivi.join(', ')}${codice ? ' · scritto in codice' : ''}.`
  v.append(perche)
  for (const c of coperti) c.hidden = true
  if (b.some((/** @type {any} */ x) => MAI_APRIBILE.has(String(x.categoria)))) {
    const n = document.createElement('p')
    n.className = 'nota'
    n.textContent = 'Questo contenuto non si apre: lo valuterà un moderatore.'
    v.append(n)
  } else {
    const apri = document.createElement('button')
    apri.type = 'button'
    apri.textContent = 'Mostra comunque'
    apri.addEventListener('click', () => { for (const c of coperti) c.hidden = false; v.remove() })
    v.append(apri)
  }
  return v
}

// ─── Persone bloccate (I-66) ──────────────────────────────────────────────────
// L'elenco sta solo su questo dispositivo e si applica nel lato Rust: i contenuti di chi blocchi non arrivano
// nemmeno qui. Qui si chiede il blocco e si tolgono dallo schermo i contenuti già mostrati.

/** Una chiave pubblica d'identità valida (64 esadecimali minuscoli): solo così entra in un selettore. */
const CHIAVE = /^[0-9a-f]{64}$/

/** Un tasto che chiede conferma con un secondo tocco (niente finestre di dialogo del browser).
 * @param {string} testo @param {string} conferma @param {() => Promise<void>} azione */
function tastoConConferma(testo, conferma, azione) {
  const b = document.createElement('button')
  b.type = 'button'
  b.className = 'tasto-azione'
  b.textContent = testo
  let armato = false
  /** @type {ReturnType<typeof setTimeout>|null} */ let scade = null
  b.addEventListener('click', async ev => {
    ev.stopPropagation()
    if (!armato) {
      armato = true
      b.textContent = conferma
      b.classList.add('armato')
      scade = setTimeout(() => { armato = false; b.textContent = testo; b.classList.remove('armato') }, 4000)
      return
    }
    if (scade) clearTimeout(scade)
    b.disabled = true
    try { await azione() } finally { b.disabled = false; armato = false; b.textContent = testo; b.classList.remove('armato') }
  })
  return b
}

/** Il tasto "Blocca" per l'autore di un contenuto; niente per i tuoi contenuti. @param {string} autore */
function tastoBlocca(autore) {
  if (!CHIAVE.test(autore) || (identita && identita.identity === autore)) return null
  return tastoConConferma('Blocca', 'Tocca ancora per bloccare', () => blocca(autore))
}

/** @param {string} autore */
async function blocca(autore) {
  try {
    await invoke('blocca', { chiave: autore })
  } catch (e) {
    mostra('bloccati-stato', String(e))
    return
  }
  for (const el of document.querySelectorAll(`[data-autore="${autore}"]`)) el.remove()
  conversazioni = conversazioni.filter(c => c.identita !== autore)
  if (discAutore === autore) vediElenco()
  disegnaPosta()
  segnaVuoto()
  await caricaBloccati()
}

async function caricaBloccati() {
  /** @type {string[]} */ const elenco = await invoke('bloccati').catch(() => [])
  const lista = document.getElementById('bloccati-lista')
  if (lista) {
    lista.replaceChildren(...elenco.filter(k => CHIAVE.test(String(k))).map(k => {
      const li = document.createElement('li')
      const c = document.createElement('span')
      c.className = 'chiave cresce'
      c.textContent = String(k)
      const b = document.createElement('button')
      b.type = 'button'
      b.className = 'tasto contorno piccolo'
      b.textContent = 'Sblocca'
      b.addEventListener('click', async () => {
        try { await invoke('sblocca', { chiave: String(k) }); mostra('bloccati-stato', 'sbloccata ✓ (i suoi contenuti tornano al prossimo aggiornamento)') } catch (e) { mostra('bloccati-stato', String(e)) }
        await caricaBloccati()
      })
      li.append(c, b)
      return li
    }))
  }
  nascondi('bloccati-vuoto', elenco.length > 0)
}

// ─── Segnalazioni (I-67) ──────────────────────────────────────────────────────
// Si segnala un CONTENUTO per un motivo dell'elenco chiuso. La firma la fa il lato Rust; la segnalazione resta nel
// nodo (non la vede chi è segnalato) e non toglie niente da sola: la valuta chi gestisce il nodo.

const MOTIVI_SEGNALAZIONE = /** @type {Array<[string, string]>} */ ([
  ['spam', 'Spam'], ['truffa', 'Truffa'], ['odio', "Odio o discriminazione"], ['violenza', 'Violenza o minacce'],
  ['armi', 'Armi'], ['droga', 'Droghe'], ['sfruttamento-minori', 'Sfruttamento di minori'], ['terrorismo', 'Terrorismo'], ['altro', 'Altro'],
])

/** "Segnala" su un contenuto di un altro (serve un'identità per firmare). @param {string} id @param {string} autore */
function tastoSegnala(id, autore) {
  if (!identita || !CHIAVE.test(id) || identita.identity === autore) return null
  const box = document.createElement('span')
  box.className = 'segnala'
  const b = document.createElement('button')
  b.type = 'button'
  b.className = 'tasto-azione'
  b.textContent = 'Segnala'
  const pannello = document.createElement('span')
  pannello.className = 'pannello-segnala'
  pannello.hidden = true
  const scelta = document.createElement('select')
  scelta.className = 'campo piccolo'
  scelta.setAttribute('aria-label', 'Motivo della segnalazione')
  for (const [valore, nome] of MOTIVI_SEGNALAZIONE) {
    const o = document.createElement('option')
    o.value = valore
    o.textContent = nome
    scelta.append(o)
  }
  const invia = document.createElement('button')
  invia.type = 'button'
  invia.className = 'tasto contorno piccolo'
  invia.textContent = 'Invia segnalazione'
  const stato = document.createElement('span')
  stato.className = 'tenue'
  stato.setAttribute('aria-live', 'polite')
  pannello.append(scelta, invia, stato)
  b.addEventListener('click', ev => { ev.stopPropagation(); pannello.hidden = !pannello.hidden })
  invia.addEventListener('click', async ev => {
    ev.stopPropagation()
    const categoria = String(scelta.value)
    if (!MOTIVI_SEGNALAZIONE.some(([v]) => v === categoria)) return
    invia.disabled = true
    stato.textContent = 'firmo e invio al nodo…'
    try {
      await invoke('segnala', { id, categoria })
      stato.textContent = 'Segnalato ✓ Lo vede solo chi gestisce il nodo, non chi hai segnalato.'
      scelta.disabled = true
    } catch (e) {
      stato.textContent = String(e)
      invia.disabled = false
    }
  })
  box.append(b, pannello)
  return box
}

// ─── Cancellare (I-68) ────────────────────────────────────────────────────────
// "Elimina" sui TUOI contenuti: il lato Rust firma la cancellazione e il nodo toglie il contenuto (solo se è tuo).
// L'account: si scrive CANCELLA; le chiavi lasciano il dispositivo solo dopo il sì del nodo.

/** "Elimina" su un tuo contenuto; `tolto` lo toglie dallo schermo quando il nodo ha detto sì.
 * @param {string} id @param {string} autore @param {() => void} tolto */
function tastoElimina(id, autore, tolto) {
  if (!identita || identita.identity !== autore || !CHIAVE.test(id)) return null
  const box = document.createElement('span')
  box.className = 'segnala'
  const stato = document.createElement('span')
  stato.className = 'tenue'
  stato.setAttribute('aria-live', 'polite')
  const b = tastoConConferma('Elimina', 'Tocca ancora per eliminare', async () => {
    stato.textContent = 'firmo e chiedo al nodo di toglierlo…'
    try {
      await invoke('cancella', { id })
      stato.textContent = ''
      tolto()
    } catch (e) {
      stato.textContent = String(e)
    }
  })
  box.append(b, stato)
  return box
}

async function cancellaAccount(/** @type {boolean} */ soloQui) {
  const c = campo('account-conferma')
  const bottoni = ['account-cancella', 'account-dimentica'].map(id => /** @type {HTMLButtonElement|null} */ (document.getElementById(id)))
  bottoni.forEach(b => { if (b) b.disabled = true })
  const fase = soloQui ? null : setInterval(async () => {
    const a = await invoke('azioni_avanzamento').catch(() => null)
    if (a) mostra('account-stato', testoFase(a))
  }, 1000)
  mostra('account-stato', soloQui ? 'tolgo le chiavi da questo dispositivo…' : 'firmo e chiedo al nodo di togliere tutto…')
  try {
    await invoke(soloQui ? 'identita_dimentica' : 'account_cancella', { conferma: c.value })
    c.value = ''
    mostra('account-stato', soloQui ? 'Identità tolta da questo dispositivo ✓' : 'Account cancellato ✓ Il nodo ha tolto i tuoi contenuti e le chiavi hanno lasciato questo dispositivo.')
    mostraMioNome(null)
    profiloChiesto = false
    postaChiesta = false
    conversazioni = []
    disegnaPosta()
    await aggiornaIdentita()
    await caricaBloccati()
  } catch (e) {
    mostra('account-stato', String(e))
  } finally {
    if (fase) clearInterval(fase)
    bottoni.forEach(b => { if (b) b.disabled = false })
  }
}

// ─── Mi piace (I-69) ──────────────────────────────────────────────────────────
// Il conto arriva dal lato Rust (reazioni verificate, una per persona). Mettere: il lato Rust firma; togliere:
// cancellazione della TUA reazione (I-68). Il "mi piace" è pubblico come un post: chi guarda il nodo vede chi l'ha messo.

/** Il tasto ♥ con il conto. Senza identità si vede solo il conto. @param {any} c contenuto con id, mi_piace, mio_mi_piace */
function tastoMiPiace(c) {
  const id = String(c.id)
  if (!CHIAVE.test(id)) return null
  let quante = Number(c.mi_piace) || 0
  let mia = typeof c.mio_mi_piace === 'string' && CHIAVE.test(c.mio_mi_piace) ? c.mio_mi_piace : null
  const b = document.createElement('button')
  b.type = 'button'
  b.className = 'tasto-azione mi-piace'
  const disegna = () => {
    b.textContent = `${mia ? '♥' : '♡'} ${quante}`
    b.setAttribute('aria-pressed', mia ? 'true' : 'false')
    b.setAttribute('aria-label', mia ? `Ti piace (${quante}). Tocca per togliere` : `Mi piace (${quante})`)
  }
  disegna()
  if (!identita) { b.disabled = true; b.title = 'Serve un\'identità per mettere mi piace'; return b }
  b.title = 'Il mi piace è pubblico, come un post'
  b.addEventListener('click', async ev => {
    ev.stopPropagation()
    b.disabled = true
    try {
      if (mia) { await invoke('cancella', { id: mia }); mia = null; quante = Math.max(0, quante - 1) }
      else { const r = await invoke('mi_piace', { id }); if (typeof r === 'string' && CHIAVE.test(r)) { mia = r; quante += 1 } }
      disegna()
    } catch (e) {
      b.title = String(e)
    } finally { b.disabled = false }
  })
  return b
}

/** @param {any} p */
function voce(p) {
  const autore = String(p.autore)
  const li = document.createElement('li')
  if (CHIAVE.test(autore)) li.setAttribute('data-autore', autore)
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
  const coperto = velo(p, testo)
  const blocca = tastoBlocca(autore)
  const segnala = tastoSegnala(String(p.id), autore)
  const elimina = tastoElimina(String(p.id), autore, () => { li.remove(); segnaVuoto() })
  const piace = tastoMiPiace(p)
  if (piace) chi.append(piace)
  if (segnala) chi.append(segnala)
  if (blocca) chi.append(blocca)
  if (elimina) chi.append(elimina)
  corpo.append(chi, ...(coperto ? [coperto] : []), testo)
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
    const veli = f.bandierine_disponibili ? '' : ' · bandierine del nodo non disponibili'
    mostra('feed-stato', `${f.post.length} post in ${Number(f.secondi)} s${scartati}${veli}`)
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

/** @typedef {{sha256: string, mime: string, dimensione: number, sezione: string, titolo: string, pezzi: string[], misure: number[], celle: boolean}} FileMedia */
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
  if (CHIAVE.test(String(m.autore))) li.setAttribute('data-autore', String(m.autore))
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
  // I-65: con una bandierina la tessera (titolo e apertura) resta sotto il velo finché non lo apri
  const coperto = velo(m, b)
  const blocca = tastoBlocca(String(m.autore))
  const segnala = tastoSegnala(String(m.id), String(m.autore))
  const elimina = tastoElimina(String(m.id), String(m.autore), () => li.remove())
  const piace = tastoMiPiace(m)
  li.append(...(coperto ? [coperto] : []), b, ...(piace ? [piace] : []), ...(segnala ? [segnala] : []), ...(blocca ? [blocca] : []), ...(elimina ? [elimina] : []))
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
  fermaCelle()
  document.getElementById('media-schermo')?.replaceChildren()
  nascondi('media-visore', true)
}

/** @param {typeof SEZIONI[keyof typeof SEZIONI]} s */
function apriSezione(s) {
  if (sezioneAperta !== s) { chiudiVisore(); document.getElementById('media-lista')?.replaceChildren() }
  sezioneAperta = s
  mostra('t-media', s.titolo)
  mostra('media-cosa', s.file)
  nascondi('media-celle-riga', s !== SEZIONI.reels)
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
    const veli = g.bandierine_disponibili ? '' : ' · bandierine del nodo non disponibili'
    mostra('media-stato', `${lista.length} contenuti${scartati}${veli}`)
  })
}

/** @param {Media} m */
async function apriMedia(m) {
  if (m.file.celle) return apriCelle(m)
  await conFaseMedia(`scarico ${mega(Number(m.file.dimensione))} via Tor…`, async () => {
    /** @type {ArrayBuffer} */
    // i pezzi tornano al lato Rust così come sono arrivati dalla scheda verificata: lì si controllano hash e numero
    const byte = await invoke('media_file', { sha256: m.file.sha256, mime: m.file.mime, dimensione: m.file.dimensione, pezzi: Array.isArray(m.file.pezzi) ? m.file.pezzi : [] })
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
/** il file scelto è stato codificato a celle (alveare): si pubblica con `celle_pubblica` */
let cellePronte = false

// ─── Alveare: foto e reel a celle ─────────────────────────────────────────────
// Qui si fa solo quello che il lato Rust non può fare: leggere i pixel dal file scelto (il browser decodifica JPEG,
// PNG e MP4) e disegnarli su un canvas. Al lato Rust vanno pixel grezzi RGBA (mai il file né il nome); da lì tornano
// pixel grezzi. Codifica, pezzi, hash, firme e rete stanno di là.

/** Lato lungo al massimo di una foto a celle (una foto da 2 MP costa ~25 MB di memoria in codifica). */
const CELLE_FOTO_LATO = 1600
/** Un reel a celle: lato lungo, fotogrammi al secondo e secondi al massimo (la codifica costa ~0,3 s a fotogramma). */
const CELLE_REEL_LATO = 480
const CELLE_REEL_FPS = 12
const CELLE_REEL_SECONDI = 30

/** Intestazione dei pixel per `celle_prepara`: larghezza, altezza, fotogrammi, fotogrammi al secondo (0 = foto). */
function testaPixel(/** @type {number} */ w, /** @type {number} */ h, /** @type {number} */ n, /** @type {number} */ fps) {
  const b = new Uint8Array(16 + w * h * 4 * n)
  new DataView(b.buffer).setUint32(0, w, true)
  new DataView(b.buffer).setUint32(4, h, true)
  new DataView(b.buffer).setUint32(8, n, true)
  new DataView(b.buffer).setUint32(12, fps, true)
  return b
}

/** Misure ridotte perché il lato lungo non superi `lato` (pari, per i colori dimezzati). */
function ridotte(/** @type {number} */ w, /** @type {number} */ h, /** @type {number} */ lato) {
  const k = Math.min(1, lato / Math.max(w, h))
  return [Math.max(2, Math.round(w * k / 2) * 2), Math.max(2, Math.round(h * k / 2) * 2)]
}

/** I pixel RGBA di una foto scelta, ridotta: dal decodificatore del browser al canvas. */
async function pixelFoto(/** @type {File} */ f) {
  const bm = await createImageBitmap(f)
  const [w, h] = ridotte(bm.width, bm.height, CELLE_FOTO_LATO)
  const c = document.createElement('canvas'); c.width = w; c.height = h
  const ctx = c.getContext('2d'); if (!ctx) throw new Error('canvas non disponibile')
  ctx.drawImage(bm, 0, 0, w, h); bm.close()
  const b = testaPixel(w, h, 1, 0)
  b.set(ctx.getImageData(0, 0, w, h).data, 16)
  return b
}

/** I fotogrammi RGBA di un video scelto, ridotti, a passo fisso (il browser decodifica, qui si disegna). */
async function pixelVideo(/** @type {File} */ f) {
  const url = URL.createObjectURL(f)
  try {
    const v = document.createElement('video'); v.muted = true; v.playsInline = true; v.preload = 'auto'; v.src = url
    await new Promise((ok, no) => { v.onloadedmetadata = () => ok(null); v.onerror = () => no(new Error('video non leggibile da questo computer (serve MP4 H.264)')) })
    if (!(v.videoWidth > 0) || !(v.duration > 0)) throw new Error('video senza immagini')
    const secondi = Math.min(v.duration, CELLE_REEL_SECONDI)
    const n = Math.max(1, Math.floor(secondi * CELLE_REEL_FPS))
    const [w, h] = ridotte(v.videoWidth, v.videoHeight, CELLE_REEL_LATO)
    const c = document.createElement('canvas'); c.width = w; c.height = h
    const ctx = c.getContext('2d'); if (!ctx) throw new Error('canvas non disponibile')
    const b = testaPixel(w, h, n, CELLE_REEL_FPS)
    for (let i = 0; i < n; i++) {
      mostra('media-pulito', `leggo il fotogramma ${i + 1} di ${n}…`)
      await new Promise((ok, no) => { v.onseeked = () => ok(null); v.onerror = () => no(new Error('video interrotto')); v.currentTime = Math.min(v.duration, i / CELLE_REEL_FPS) })
      ctx.drawImage(v, 0, 0, w, h)
      b.set(ctx.getImageData(0, 0, w, h).data, 16 + i * w * h * 4)
    }
    v.removeAttribute('src'); v.load()
    return { b, n }
  } finally { URL.revokeObjectURL(url) }
}

/** Codifica a celle il file scelto (foto o video) nel lato Rust e dice quanto pesa. */
async function preparaCelle(/** @type {File} */ f) {
  const video = f.type.startsWith('video/')
  mostra('media-pulito', video ? 'leggo i fotogrammi…' : 'leggo la foto…')
  const pix = video ? (await pixelVideo(f)).b : await pixelFoto(f)
  mostra('media-pulito', 'codifico a celle sul tuo computer…')
  const fase = setInterval(async () => {
    const a = await invoke('celle_avanzamento').catch(() => null)
    if (a && a.fase === 'codifico') mostra('media-pulito', testoFase(a))
  }, 1000)
  try {
    const p = await invoke('celle_prepara', pix)
    const fotogrammi = Number(p.fotogrammi) ? ` · ${Number(p.fotogrammi)} fotogrammi` : ''
    mostra('media-pulito', `alveare ✓ · ${Number(p.pezzi)} pezzi · anteprima ${mega(Number(p.anteprima))} · in tutto ${mega(Number(p.dimensione))}${fotogrammi} · ${(Number(p.millisecondi) / 1000).toFixed(1)} s`)
  } finally { clearInterval(fase) }
}

/** @type {number|null} */ let celleTimer = null
/** @type {number|null} */ let celleQuadro = null

function fermaCelle() {
  if (celleTimer !== null) { clearInterval(celleTimer); celleTimer = null }
  if (celleQuadro !== null) { clearInterval(celleQuadro); celleQuadro = null }
}

/** Disegna sul canvas i pixel grezzi (larghezza, altezza, RGBA) tornati da `celle_quadro`. */
function disegna(/** @type {HTMLCanvasElement} */ c, /** @type {ArrayBuffer} */ buf) {
  if (buf.byteLength < 8) return false
  const dv = new DataView(buf)
  const w = dv.getUint32(0, true), h = dv.getUint32(4, true)
  if (w === 0 || h === 0 || buf.byteLength !== 8 + w * h * 4) return false
  if (c.width !== w || c.height !== h) { c.width = w; c.height = h }
  const ctx = c.getContext('2d'); if (!ctx) return false
  ctx.putImageData(new ImageData(new Uint8ClampedArray(buf, 8, w * h * 4), w, h), 0, 0)
  return true
}

/** Apre una foto o un reel a celle: l'immagine intera compare dal primo pezzo e si affina mentre gli altri scendono. */
async function apriCelle(/** @type {Media} */ m) {
  await conFaseMedia('cerco i pezzi via Tor…', async () => {
    chiudiVisore()
    // la scheda torna al lato Rust così com'è arrivata: lì si controllano hash e misure di ogni pezzo
    await invoke('celle_apri', { sha256: m.file.sha256, mime: m.file.mime, dimensione: m.file.dimensione, pezzi: Array.isArray(m.file.pezzi) ? m.file.pezzi : [], misure: Array.isArray(m.file.misure) ? m.file.misure : [] })
    const c = document.createElement('canvas'); c.className = 'celle'
    document.getElementById('media-schermo')?.append(c)
    mostra('media-v-titolo', String(m.file.titolo) || 'Senza titolo')
    mostra('media-v-chi', `${m.nome ? String(m.nome) : String(m.autore).slice(0, 12) + '…'} · ${quando(Number(m.creato))} · alveare`)
    mostra('media-v-testo', String(m.testo))
    nascondi('media-visore', false)
    document.getElementById('media-visore')?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    const reel = String(m.file.mime).startsWith('video/')
    let disegnati = 0, quadro = 0, fotogrammi = 0, fps = CELLE_REEL_FPS
    celleTimer = setInterval(async () => {
      const s = await invoke('celle_stato').catch(() => null)
      if (!s) return
      fotogrammi = Number(s.fotogrammi); if (Number(s.fps) > 0) fps = Number(s.fps)
      const arrivati = Number(s.pezzi_arrivati), totali = Number(s.pezzi_totali)
      const dove = arrivati >= totali ? `${totali} pezzi · ${mega(Number(s.byte_arrivati))} ✓` : `pezzo ${arrivati} di ${totali} · ${mega(Number(s.byte_arrivati))}`
      if (s.errore) mostra('media-stato', `${dove} · ${String(s.errore)}`)
      else if (!reel && arrivati === 0) { const a = await invoke('celle_avanzamento').catch(() => null); mostra('media-stato', a ? testoFase(a) || 'cerco il primo pezzo…' : 'cerco il primo pezzo…') }
      else mostra('media-stato', reel ? `${dove} · ${fotogrammi} fotogrammi` : arrivati === 1 ? `anteprima: ${dove} (immagine intera, si affina)` : dove)
      // la foto si ridisegna a ogni pezzo in più; il reel scorre con il suo timer
      if (!reel && fotogrammi > 0 && arrivati !== disegnati) {
        if (disegna(c, await invoke('celle_quadro', { indice: 0 }))) disegnati = arrivati
      }
      if (s.finito && (reel || arrivati === disegnati)) { if (celleTimer !== null) clearInterval(celleTimer); celleTimer = null }
    }, 400)
    if (reel) {
      celleQuadro = setInterval(async () => {
        if (fotogrammi === 0) return
        if (quadro >= fotogrammi) { if (celleTimer === null) quadro = 0; else return }
        if (disegna(c, await invoke('celle_quadro', { indice: quadro }))) quadro++
      }, Math.round(1000 / fps))
    }
  })
}

async function sceltoFile() {
  const scegli = /** @type {HTMLInputElement} */ (document.getElementById('media-scegli'))
  const f = scegli.files && scegli.files[0]
  filePronto = false
  cellePronte = false
  const pub = /** @type {HTMLButtonElement|null} */ (document.getElementById('media-pubblica'))
  if (pub) pub.disabled = true
  mostra('media-pulito', '')
  if (!f) return
  if (f.size > 1024 * 1024 * 1024) { mostra('media-pulito', 'file troppo grande (massimo 1 GB)'); return }
  const alveare = /** @type {HTMLInputElement|null} */ (document.getElementById('media-celle'))
  if (alveare && alveare.checked && sezioneAperta === SEZIONI.reels && (f.type === 'image/jpeg' || f.type === 'image/png' || f.type === 'video/mp4' || f.type === 'video/quicktime')) {
    try {
      await preparaCelle(f)
      cellePronte = true
      filePronto = true
      if (pub) pub.disabled = false
    } catch (e) {
      mostra('media-pulito', String(e))
    }
    return
  }
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
  await conFaseMedia(cellePronte ? 'firmo, carico i pezzi e cerco il nodo…' : 'firmo, carico il file e cerco il nodo…', async () => {
    await invoke(cellePronte ? 'celle_pubblica' : 'media_pubblica', { sezione: s.nome, titolo: campo('media-titolo').value, testo: campo('media-testo').value })
    fatto = true
    filePronto = false
    cellePronte = false
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
/** chi ha aperto la discussione aperta @type {string|null} */ let discAutore = null

/** @param {Discussione} d */
function chiDi(d) {
  return d.nome ? String(d.nome) : `${String(d.autore).slice(0, 12)}…`
}

/** @param {Discussione} d */
function voceDiscussione(d) {
  const li = document.createElement('li')
  if (CHIAVE.test(String(d.autore))) li.setAttribute('data-autore', String(d.autore))
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
  // I-65: con una bandierina titolo, anteprima e apertura restano sotto il velo finché non lo apri
  const coperto = velo(d, b)
  li.append(...(coperto ? [coperto] : []), b)
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
  discAutore = null
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
    const veli = f.bandierine_disponibili ? '' : ' · bandierine del nodo non disponibili'
    mostra('forum-stato', `${lista.length} discussioni${scartati}${veli}`)
    vediElenco()
  })
}

async function apriDiscussione(/** @type {string} */ id) {
  await conFaseForum('apro la discussione…', async () => {
    const d = await invoke('forum_discussione', { id })
    if (!d) { mostra('forum-stato', 'discussione non trovata sul nodo'); return }
    discAperta = String(d.discussione.id)
    discAutore = String(d.discussione.autore)
    mostra('disc-titolo', String(d.discussione.titolo) || 'Senza titolo')
    mostra('disc-chi', `${chiDi(d.discussione)} · ${quando(Number(d.discussione.creato))}`)
    const bd = tastoBlocca(discAutore)
    const sd = tastoSegnala(discAperta, discAutore)
    const ed = tastoElimina(discAperta, discAutore, () => { vediElenco(); caricaForum() })
    document.getElementById('disc-azioni')?.replaceChildren(...(sd ? [sd] : []), ...(bd ? [bd] : []), ...(ed ? [ed] : []))
    mostra('disc-testo', String(d.discussione.testo))
    document.getElementById('disc-risposte')?.replaceChildren(...d.risposte.map(voce))
    const veli = d.bandierine_disponibili ? '' : ' · bandierine del nodo non disponibili'
    mostra('forum-stato', (d.risposte.length === 1 ? '1 risposta' : `${d.risposte.length} risposte`) + veli)
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

// Nodo dei file (hub): dove vanno reels, film e musica. Facoltativo: senza, i file stanno sul nodo.
async function aggiornaNodoFile() {
  const si = await invoke('nodo_file_impostato').catch(() => false)
  mostra('nodo-file-attuale', si ? 'I file vanno a un nodo dei file separato. Puoi cambiarlo, o tornare al tuo nodo.' : 'I file (reels, film, musica) vanno al tuo nodo. Se c\'è un nodo dei file con più banda, scrivilo qui.')
  nascondi('nodo-file-togli', !si)
}

async function salvaNodoFile(/** @type {boolean} */ togli) {
  const c = campo('nodo-file-indirizzo')
  try {
    await invoke('nodo_file_imposta', { indirizzo: togli ? '' : c.value })
    c.value = ''
    mostra('nodo-file-stato', togli ? 'i file tornano al tuo nodo ✓' : 'nodo dei file impostato ✓')
    await aggiornaNodoFile()
  } catch (e) {
    mostra('nodo-file-stato', String(e))
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
  const bf = c.identita ? tastoBlocca(String(c.identita)) : null
  document.getElementById('filo-azioni')?.replaceChildren(...(bf ? [bf] : []))
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
  caricaBloccati()
  document.getElementById('account-cancella')?.addEventListener('click', () => cancellaAccount(false))
  document.getElementById('account-dimentica')?.addEventListener('click', () => cancellaAccount(true))
  document.getElementById('nodo-file-salva')?.addEventListener('click', () => salvaNodoFile(false))
  document.getElementById('nodo-file-togli')?.addEventListener('click', () => salvaNodoFile(true))
  aggiornaNodoFile()
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
