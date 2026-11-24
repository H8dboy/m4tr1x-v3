// @ts-check
'use strict'
/**
 * Metriche del nodo (F2b, I-43): SOLO contatori con nomi fissi.
 * Nessun IP, nessuna pubkey, nessun onion, nessun id di eventi o blob, nessun testo di chi usa il nodo:
 * dentro ci sono solo numeri e un booleano, e un motivo di rifiuto entra solo come CATEGORIA fissa
 * (la parola prima dei due punti di NIP-01, se è in elenco; altrimenti "altro").
 * Esposte solo sul listener del proprietario con la chiave admin K13 (http.js), mai su quello dell'onion.
 * Un sequestro del nodo non deve dare più di quello che è già pubblico (PEZZI_MANCANTI §1).
 */

/** Categorie di rifiuto ammesse (prefissi NIP-01 usati dal relay). */
const CATEGORIE = /** @type {const} */ (['invalid', 'blocked', 'rate-limited', 'restricted', 'error'])
const conta0 = () => /** @type {Record<string, number>} */ (Object.fromEntries([...CATEGORIE, 'altro'].map(c => [c, 0])))

/** @param {string} motivo @returns {string} solo un nome dell'elenco */
function categoria(motivo) {
  const p = String(motivo || '').split(':')[0].trim()
  return /** @type {readonly string[]} */ (CATEGORIE).includes(p) ? p : 'altro'
}

/** Un intero non negativo, altrimenti 0: nelle metriche non entra nient'altro. @param {any} n */
const intero = n => (Number.isSafeInteger(n) && n >= 0 ? n : 0)

function creaMetriche() {
  const avvio = Date.now()
  const eventi = { client: { accettati: 0, duplicati: 0, rifiutati: conta0() }, peer: { accettati: 0, duplicati: 0, rifiutati: conta0() } }
  const blob = { ricevuti: 0, byte_ricevuti: 0, rifiutati: 0 }

  return {
    /**
     * Esito di un evento (dal relay). Il motivo serve solo a scegliere la categoria e non si conserva.
     * @param {boolean} ok @param {string} motivo @param {boolean} daPeer
     */
    evento(ok, motivo, daPeer) {
      const e = daPeer ? eventi.peer : eventi.client
      if (ok) { if (String(motivo).startsWith('duplicate')) e.duplicati++; else e.accettati++ }
      else e.rifiutati[categoria(motivo)]++
    },
    /** @param {number} byte */
    blobRicevuto(byte) { blob.ricevuti++; blob.byte_ricevuti += intero(byte) },
    blobRifiutato() { blob.rifiutati++ },
    /**
     * Fotografia per l'admin: solo numeri e booleani.
     * @param {{ eventi: number, blob: number, byteBlob: number, connessioni: number, tor: boolean }} stato
     */
    istantanea(stato) {
      const copia = (/** @type {typeof eventi.client} */ e) => ({ accettati: e.accettati, duplicati: e.duplicati, rifiutati: { ...e.rifiutati } })
      return {
        attivo_da_sec: Math.floor((Date.now() - avvio) / 1000),
        eventi: { client: copia(eventi.client), peer: copia(eventi.peer) },
        blob: { ...blob },
        archivio: { eventi: intero(stato.eventi), blob: intero(stato.blob), byte_blob: intero(stato.byteBlob) },
        connessioni_relay: intero(stato.connessioni),
        tor: stato.tor === true,
      }
    },
  }
}

module.exports = { creaMetriche, categoria, CATEGORIE }
