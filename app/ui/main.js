// @ts-check
'use strict'
// Interfaccia di M4TR1X. Regole (I-31, I-01):
// - nessuna chiave qui: firme e segreti restano nel lato Rust;
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

async function aggiorna() {
  try {
    const s = await invoke('stato')
    mostra('versione', String(s.versione))
    mostra('trasporto', NOMI[/** @type {keyof typeof NOMI} */ (s.trasporto)] || '?')
    mostra('percento', s.trasporto === 'avvio' ? `${Number(s.percento)}%` : '')
  } catch {
    mostra('trasporto', 'non raggiungibile')
  }
}

document.addEventListener('DOMContentLoaded', aggiorna)
