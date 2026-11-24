// @ts-check
'use strict'
/**
 * @m4tr1x/moderazione — bandierine sui CONTENUTI pubblici (I-65, docs/design/MODERAZIONE.md).
 *
 * - Guarda solo i kind pubblici di testo (KIND_PUBBLICI): per ogni altro kind esce PRIMA di leggere il testo.
 *   Buste, sigilli e annunci dei messaggi privati non arrivano mai al confronto.
 * - Non riceve e non restituisce chi ha scritto: del contenuto usa solo kind e testo. Nessun contatore,
 *   elenco o punteggio per persona.
 * - Linguaggio in codice: il testo si normalizza (numeri e simboli al posto di lettere, lettere separate,
 *   lettere di altri alfabeti uguali a vista, caratteri invisibili, accenti, lettere ripetute) prima del confronto;
 *   una regola trovata solo nella forma normalizzata è «scritta in codice» e pesa di più.
 * - "Cose strane" senza elenco: caratteri invisibili e alfabeti mischiati nella stessa parola.
 * - Non toglie niente: restituisce bandierine. Togliere è una decisione umana (procedura DSA).
 * L'elenco vero delle regole NON sta nel repo: arriva a chi lo usa (validaRegole). Nessuna dipendenza, nessuna rete.
 */

/** Kind analizzati: post, commenti/topic (NIP-22), articoli (NIP-23). Nient'altro. */
const KIND_PUBBLICI = Object.freeze([1, 1111, 30023])
/** Categorie delle regole. `offuscamento` è riservata ai segnali senza elenco. */
const CATEGORIE = Object.freeze(['truffa', 'spam', 'odio', 'violenza', 'armi', 'droga', 'sfruttamento-minori', 'terrorismo', 'altro'])
const OFFUSCAMENTO = 'offuscamento'
const MASSIMO_TESTO = 64 * 1024
const MASSIMO_REGOLE = 5000
/** Sotto questa lunghezza una regola vale solo come parola intera, mai "senza spazi" (falsi positivi). */
const MINIMO_COMPATTO = 5

const INVISIBILI = /[­​-‏⁠-⁤﻿]/g
/** Lettere di altri alfabeti uguali (a vista) a lettere latine. */
const SOSIA = /** @type {Record<string, string>} */ ({
  'а': 'a', 'в': 'b', 'е': 'e', 'ё': 'e', 'к': 'k', 'м': 'm', 'н': 'h', 'о': 'o', 'р': 'p', 'с': 'c', 'т': 't', 'у': 'y', 'х': 'x',
  'і': 'i', 'ї': 'i', 'ј': 'j', 'ѕ': 's', 'ԁ': 'd', 'ӏ': 'l', 'ԛ': 'q', 'ԝ': 'w',
  'α': 'a', 'β': 'b', 'ε': 'e', 'η': 'n', 'ι': 'i', 'κ': 'k', 'ν': 'v', 'ο': 'o', 'ρ': 'p', 'τ': 't', 'υ': 'u', 'χ': 'x', 'ω': 'w',
})
/** Numeri e simboli usati al posto di lettere. */
const LEET = /** @type {Record<string, string>} */ ({
  '0': 'o', '1': 'i', '3': 'e', '4': 'a', '5': 's', '7': 't', '8': 'b', '9': 'g', '@': 'a', '$': 's', '€': 'e', '!': 'i', '|': 'i', '+': 't',
})

/** Testo "in chiaro": minuscolo, senza accenti, solo parole. Niente NFKC: lettere larghe o "matematiche" sono
 * un trucco, e qui devono restare diverse. @param {string} s */
function inChiaro(s) {
  return ' ' + s.toLowerCase().normalize('NFD').replace(/\p{M}/gu, '').normalize('NFC')
    .replace(/[^\p{L}\p{N}]+/gu, ' ').replace(/(\p{L})\1+/gu, '$1').trim() + ' '
}

/**
 * Forma normalizzata: `parole` (con gli spazi tra le parole) e `compatto` (senza nessun separatore).
 * @param {string} s @returns {{ parole: string, compatto: string }}
 */
function normalizza(s) {
  let t = s.normalize('NFKC').toLowerCase().replace(INVISIBILI, '')
  t = t.normalize('NFD').replace(/\p{M}/gu, '').normalize('NFC')
  t = Array.from(t, c => SOSIA[c] ?? LEET[c] ?? c).join('')
  t = t.replace(/[^a-z]+/g, ' ').replace(/([a-z])\1+/g, '$1').trim()
  return { parole: ' ' + t + ' ', compatto: t.replace(/ /g, '') }
}

/** Segnali che non dipendono da nessun elenco. @param {string} s @returns {string[]} */
function segnaliStrani(s) {
  const out = []
  if (/[\p{L}\p{N}][­​-‏⁠-⁤﻿]+[\p{L}\p{N}]/u.test(s)) out.push('invisibili')
  for (const w of s.normalize('NFKC').match(/\p{L}+/gu) || []) {
    const latino = /\p{Script=Latin}/u.test(w), altro = /[\p{Script=Cyrillic}\p{Script=Greek}]/u.test(w)
    if (latino && altro) { out.push('alfabeti-mischiati'); break }
  }
  return out
}

/**
 * @typedef {{ id: string, categoria: string, peso: number, parole: string, compatto: string }} Regola
 * @typedef {{ versione: number, regole: Regola[] }} Regole
 * @typedef {{ categoria: string, peso: number, regole: string[], codice: boolean }} Bandierina
 */

/**
 * Controlla e prepara un elenco di regole `{ versione, regole: [{ id, categoria, schema, peso }] }`.
 * Un elenco storto si rifiuta tutto (eccezione): meglio nessuna regola che regole a metà.
 * @param {any} elenco @returns {Regole}
 */
function validaRegole(elenco) {
  const no = (/** @type {string} */ m) => { throw new Error(`regole non valide: ${m}`) }
  if (!elenco || typeof elenco !== 'object' || Array.isArray(elenco)) no('non è un oggetto')
  const extra = Object.keys(elenco).filter(k => k !== 'versione' && k !== 'regole')
  if (extra.length) no(`campi non previsti (${extra.join(', ')})`)
  if (!Number.isSafeInteger(elenco.versione) || elenco.versione < 1) no('versione')
  if (!Array.isArray(elenco.regole) || elenco.regole.length > MASSIMO_REGOLE) no('elenco')
  const visti = new Set()
  const regole = elenco.regole.map((/** @type {any} */ r, /** @type {number} */ i) => {
    if (!r || typeof r !== 'object' || Object.keys(r).some(k => !['id', 'categoria', 'schema', 'peso'].includes(k))) no(`regola ${i}: forma`)
    if (typeof r.id !== 'string' || !/^[a-z0-9-]{1,32}$/.test(r.id) || visti.has(r.id)) no(`regola ${i}: id`)
    visti.add(r.id)
    if (!CATEGORIE.includes(r.categoria)) no(`regola ${r.id}: categoria`)
    if (!Number.isInteger(r.peso) || r.peso < 1 || r.peso > 3) no(`regola ${r.id}: peso`)
    if (typeof r.schema !== 'string' || r.schema.length > 100) no(`regola ${r.id}: schema`)
    const n = normalizza(r.schema)
    if (n.compatto.length < 3) no(`regola ${r.id}: schema troppo corto`)
    return { id: r.id, categoria: r.categoria, peso: r.peso, parole: n.parole, compatto: n.compatto }
  })
  return { versione: elenco.versione, regole }
}

/**
 * Bandierine di un evento pubblico. Usa SOLO `kind` e `content`.
 * @param {{ kind: number, content: string }} evento @param {Regole} regole @returns {Bandierina[]}
 */
function analizza(evento, regole) {
  const kind = evento && evento.kind
  if (!KIND_PUBBLICI.includes(kind)) return []
  const testo = typeof evento.content === 'string' ? evento.content.slice(0, MASSIMO_TESTO) : ''
  if (!testo) return []
  /** @type {Map<string, Bandierina>} */ const per = new Map()
  const aggiungi = (/** @type {string} */ categoria, /** @type {number} */ peso, /** @type {string} */ id, /** @type {boolean} */ codice) => {
    const b = per.get(categoria) || { categoria, peso: 0, regole: [], codice: false }
    b.peso = Math.max(b.peso, peso)
    if (!b.regole.includes(id)) b.regole.push(id)
    b.codice = b.codice || codice
    per.set(categoria, b)
  }
  for (const s of segnaliStrani(testo)) aggiungi(OFFUSCAMENTO, 1, s, true)
  const chiaro = inChiaro(testo)
  const n = normalizza(testo)
  for (const r of regole.regole) {
    const trovata = n.parole.includes(r.parole) || (r.compatto.length >= MINIMO_COMPATTO && n.compatto.includes(r.compatto))
    if (!trovata) continue
    const codice = !chiaro.includes(r.parole)
    aggiungi(r.categoria, codice ? Math.min(3, r.peso + 1) : r.peso, r.id, codice)
  }
  return [...per.values()].sort((a, b) => b.peso - a.peso || a.categoria.localeCompare(b.categoria))
}

module.exports = { analizza, validaRegole, normalizza, KIND_PUBBLICI, CATEGORIE, OFFUSCAMENTO }
