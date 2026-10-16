// @ts-check
'use strict'
// Finestra del seme (I-58). Regole:
// - le parole arrivano dal lato Rust solo per essere mostrate; qui non si salvano
//   (niente storage del browser) e non si stampano; si tolgono dalla pagina appena
//   non servono più;
// - copia, taglia, incolla, trascina, selezione e menu contestuale disattivati;
// - si parla solo con i tre comandi seme_* (capability "seme").

/** @type {(cmd: string, args?: object) => Promise<any>} */
const invoke = /** @type {any} */ (window).__TAURI__.core.invoke

for (const ev of ['copy', 'cut', 'paste', 'contextmenu', 'dragstart', 'drop', 'selectstart']) {
  document.addEventListener(ev, e => {
    // nel campo del recupero si può selezionare il testo che si sta scrivendo, ma mai copiarlo o incollarlo
    if (ev === 'selectstart' && e.target instanceof HTMLTextAreaElement) return
    e.preventDefault()
  }, true)
}

const $ = (/** @type {string} */ id) => /** @type {HTMLElement} */ (document.getElementById(id))
/** @type {number[]} */ let posizioni = []

function svuota(/** @type {HTMLElement} */ el) {
  while (el.firstChild) el.removeChild(el.firstChild)
}

function mostraPasso(/** @type {string} */ id) {
  for (const p of ['passo-mostra', 'passo-verifica', 'passo-recupera']) $(p).hidden = p !== id
}

async function crea(rivedi = false) {
  const s = await invoke('seme_nuovo', { rivedi })
  posizioni = s.posizioni
  const ol = $('parole')
  svuota(ol)
  s.parole.forEach((/** @type {string} */ w) => {
    const li = document.createElement('li')
    li.textContent = w
    ol.appendChild(li)
  })
  s.parole.length = 0
  mostraPasso('passo-mostra')
}

function versoVerifica() {
  svuota($('parole')) // le parole spariscono dalla pagina durante la verifica
  const box = $('domande')
  svuota(box)
  for (const p of posizioni) {
    const l = document.createElement('label')
    l.textContent = `Parola n. ${p} `
    const i = document.createElement('input')
    i.type = 'text'
    i.autocomplete = 'off'
    i.spellcheck = false
    i.setAttribute('autocapitalize', 'off')
    l.appendChild(i)
    box.appendChild(l)
  }
  $('errore-verifica').hidden = true
  mostraPasso('passo-verifica')
  /** @type {HTMLInputElement|null} */ (box.querySelector('input'))?.focus()
}

async function conferma() {
  const campi = /** @type {HTMLInputElement[]} */ ([...$('domande').querySelectorAll('input')])
  const risposte = campi.map(c => c.value)
  campi.forEach(c => { c.value = '' })
  try {
    await invoke('seme_conferma', { risposte })
    // il lato Rust ha salvato le chiavi e chiude questa finestra
  } catch {
    $('errore-verifica').hidden = false
  }
}

async function recupera() {
  const t = /** @type {HTMLTextAreaElement} */ ($('parole-recupero'))
  const parole = t.value
  t.value = ''
  try {
    await invoke('seme_recupera', { parole })
  } catch {
    $('errore-recupero').hidden = false
  }
}

document.addEventListener('DOMContentLoaded', () => {
  const modo = new URLSearchParams(location.search).get('modo')
  const scritte = /** @type {HTMLInputElement} */ ($('scritte'))
  scritte.addEventListener('change', () => { /** @type {HTMLButtonElement} */ ($('avanti')).disabled = !scritte.checked })
  $('avanti').addEventListener('click', versoVerifica)
  $('indietro').addEventListener('click', () => { scritte.checked = false; crea(true) })
  $('conferma').addEventListener('click', conferma)
  $('recupera').addEventListener('click', recupera)
  if (modo === 'recupera') mostraPasso('passo-recupera')
  else crea(false)
})
