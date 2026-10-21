#!/usr/bin/env node
// @ts-check
'use strict'
/**
 * I-01 dinamica: guida l'app VERA con WebDriver (tauri-driver su 127.0.0.1:4444; decisione del fondatore,
 * 21/10/2026: niente scenari di prova dentro l'eseguibile). L'app gira come utente dedicato con le regole di
 * tools/i01/rete.sh; questo client gira fuori da quelle regole e parla solo con tauri-driver.
 *   1. sessione: tauri-driver avvia l'app;
 *   2. aspetta "Tor: pronto" (il tor figlio dell'app ha finito il bootstrap passando SOLO dai relay);
 *   3. "Aggiorna" il feed e aspetta il post del nodo di prova: prova che la rete via Tor è stata usata davvero
 *      (senza, contatori a 0 non dimostrerebbero niente).
 * Righe "fase ...": stesse parole delle altre prove con Tor. Uscita 0 solo se il post arriva.
 *
 * Uso: node tools/i01/prova.js <percorso dell'app>
 */
const { TESTO } = require('./nodo')

const BASE = 'http://127.0.0.1:4444'
// chiave del riferimento all'elemento nello standard W3C WebDriver ("web element identifier"); la prima
// versione l'aveva scritta a memoria e sbagliata (CI del 21/10/2026: WebKitWebDriver risponde con questa)
const ELEMENTO = 'element-6066-11e4-a52e-4f735466cecf'
const pausa = (/** @type {number} */ ms) => new Promise(r => setTimeout(r, ms))

/** @param {string} metodo @param {string} via @param {any} [corpo] */
async function wd(metodo, via, corpo) {
  const r = await fetch(`${BASE}${via}`, { method: metodo, headers: { 'content-type': 'application/json' }, body: corpo === undefined ? undefined : JSON.stringify(corpo) })
  const j = /** @type {any} */ (await r.json().catch(() => ({})))
  if (!r.ok) throw new Error(`WebDriver ${metodo} ${via.replace(/\/session\/[^/]+/, '/session/…')}: ${r.status} ${JSON.stringify(j.value || j).slice(0, 300)}`)
  return j.value
}

/** Riferimento all'elemento; se la risposta non ce l'ha, errore SUBITO con la risposta vera (CI del 21/10/2026:
 * "ok" senza riferimento, poi 300 s di attese su "undefined"). @param {string} s @param {string} css */
async function elemento(s, css) {
  const v = await wd('POST', `/session/${s}/element`, { using: 'css selector', value: css })
  const id = v && typeof v === 'object' ? v[ELEMENTO] : undefined
  if (typeof id !== 'string' || !id) throw new Error(`WebDriver: "${css}" senza riferimento all'elemento, risposta ${JSON.stringify(v).slice(0, 300)}`)
  return id
}
/** @param {string} s @param {string} css */
const testo = async (s, css) => String(await wd('GET', `/session/${s}/element/${await elemento(s, css)}/text`))

/** Aspetta che `cond` dia un valore (non falso), entro `ms`; `nome` va nel messaggio di errore. */
async function aspetta(/** @type {() => Promise<any>} */ cond, /** @type {number} */ ms, /** @type {string} */ nome) {
  const t0 = Date.now()
  let ultimo = ''
  while (Date.now() - t0 < ms) {
    try { const v = await cond(); if (v) return { valore: v, secondi: Math.round((Date.now() - t0) / 1000) } } catch (e) {
      ultimo = String(/** @type {Error} */ (e).message)
      if (ultimo.includes('senza riferimento all\'elemento')) throw e   // errore del client: aspettare non serve
    }
    await pausa(1000)
  }
  throw new Error(`${nome}: niente entro ${ms / 1000} s${ultimo ? ` (ultimo errore: ${ultimo})` : ''}`)
}

async function main() {
  const app = process.argv[2]
  if (!app) throw new Error('uso: prova.js <percorso dell\'app>')
  // tauri-driver pronto?
  await aspetta(async () => (await fetch(`${BASE}/status`)).ok, 60000, 'fase "webdriver": tauri-driver non risponde')
  const sessione = await aspetta(async () => (await wd('POST', '/session', { capabilities: { alwaysMatch: { 'tauri:options': { application: app } } } })).sessionId, 120000, 'fase "webdriver": sessione non aperta')
  const s = sessione.valore
  // cosa ha caricato la finestra (per capire un "elemento non trovato")
  const dove = async () => `pagina ${JSON.stringify(await wd('GET', `/session/${s}/url`).catch(e => String(e.message)))} titolo ${JSON.stringify(await wd('GET', `/session/${s}/title`).catch(e => String(e.message)))}`
  console.log(`sessione aperta in ${sessione.secondi} s: ${await dove()}`)
  try {
    const tor = await aspetta(async () => {
      const t = await testo(s, '#trasporto')
      if (t === 'errore') throw new Error('il trasporto dell\'app è in errore')
      return t === 'pronto' && t
    }, 300000, 'fase "tor": il tor dell\'app non è arrivato a "pronto"')
    console.log(`tor dell'app pronto in ${tor.secondi} s`)
    await wd('POST', `/session/${s}/element/${await elemento(s, '#aggiorna-feed')}/click`, {})
    const feed = await aspetta(async () => (await testo(s, '#feed-lista')).includes(TESTO), 480000, 'fase "rete": il post del nodo non è arrivato nel feed')
    console.log(`feed letto via Tor in ${feed.secondi} s: post del nodo presente`)
  } catch (e) {
    console.error(await dove().catch(() => 'pagina non leggibile'))
    try { console.error(`stato del feed: ${await testo(s, '#feed-stato')}`) } catch {}
    throw e
  } finally {
    await wd('DELETE', `/session/${s}`).catch(() => {})
  }
}

main().catch(e => { console.error(String(/** @type {Error} */ (e).message || e)); process.exit(1) })
