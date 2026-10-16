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

async function aggiornaStato() {
  try {
    const s = await invoke('stato')
    mostra('versione', String(s.versione))
    mostra('trasporto', NOMI[/** @type {keyof typeof NOMI} */ (s.trasporto)] || '?')
    mostra('percento', s.trasporto === 'avvio' ? `${Number(s.percento)}%` : '')
  } catch {
    mostra('trasporto', 'non raggiungibile')
  }
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

document.addEventListener('DOMContentLoaded', () => {
  document.getElementById('crea')?.addEventListener('click', () => apriSeme('crea'))
  document.getElementById('recupera')?.addEventListener('click', () => apriSeme('recupera'))
  aggiornaStato()
  aggiornaIdentita()
})
