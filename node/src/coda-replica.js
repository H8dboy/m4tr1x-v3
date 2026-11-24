// @ts-check
'use strict'
/**
 * Coda della replica dei file tra peer (revisione del 16/11, RIPRESA 91 f).
 * Prima ogni evento ricevuto da un peer faceva partire SUBITO la replica dei suoi file, tutti gli eventi insieme:
 * una sincronizzazione da 500 eventi apriva centinaia di richieste via Tor, e un evento con ~700 tag `pezzo`
 * inventati (sta nei 64 KB di un evento) costava ~700 × 3 tentativi. Ora:
 *   1) i file di ogni evento vanno in coda e se ne lavorano pochi insieme (`INSIEME`);
 *   2) la coda ha un tetto di file in attesa (`MASSIMO`): oltre, l'evento resta ma i suoi file non si replicano
 *      (chi guarda li scarica dal nodo d'origine);
 *   3) i file di un evento si lasciano perdere al primo che non arriva dopo tutti i tentativi (il peer non ce l'ha,
 *      o manda dati sbagliati): un evento con 700 pezzi inventati costa i tentativi di UN pezzo.
 * Nel log solo conti, mai hash né indirizzi (I-18).
 */

/** Eventi i cui file si replicano insieme (via Tor ogni file apre una richiesta verso l'onion del peer). */
const INSIEME = 2
/** File (pezzi o interi) al massimo in attesa di replica. */
const MASSIMO = 4096

/**
 * @template T
 * @param {{ scarica: (sha: string, lavoro: T) => Promise<boolean>, ce: (sha: string) => boolean, log: (s: string) => void, insieme?: number, massimo?: number }} o
 *   `scarica` torna true se il file è arrivato e verificato; `ce` dice se il file è già sul nodo.
 */
function creaCoda(o) {
  const insieme = o.insieme ?? INSIEME
  const massimo = o.massimo ?? MASSIMO
  /** @type {{ shas: string[], lavoro: T }[]} */ const coda = []
  /** @type {Set<string>} */ const inCorso = new Set()
  let attivi = 0
  let inAttesa = 0
  let fermo = false

  /** @param {{ shas: string[], lavoro: T }} e */
  async function replicaEvento(e) {
    try {
      for (const [i, sha] of e.shas.entries()) {
        if (fermo) break
        if (o.ce(sha) || inCorso.has(sha)) continue
        inCorso.add(sha)
        let preso = false
        try { preso = await o.scarica(sha, e.lavoro) } catch { preso = false } finally { inCorso.delete(sha) }
        if (!preso && !fermo) {
          const resto = e.shas.length - i - 1
          if (resto > 0) o.log(`peer: file dell'evento lasciati perdere (${resto} non chiesti: il peer non dà il precedente)`)
          break
        }
      }
    } finally {
      if (!fermo) inAttesa -= e.shas.length
    }
  }

  function avanti() {
    while (!fermo && attivi < insieme && coda.length) {
      const e = /** @type {{ shas: string[], lavoro: T }} */ (coda.shift())
      attivi++
      replicaEvento(e).finally(() => { attivi--; avanti() })
    }
  }

  return {
    /**
     * I file di un evento in coda (senza doppioni, senza quelli già sul nodo).
     * @param {string[]} shas @param {T} lavoro @returns {boolean} false se non c'era niente da fare o la coda è piena
     */
    accoda(shas, lavoro) {
      if (fermo) return false
      const da = [...new Set(shas)].filter(s => !o.ce(s))
      if (!da.length) return false
      if (inAttesa + da.length > massimo) {
        o.log(`peer: coda di replica piena (${inAttesa} file in attesa): ${da.length} file dell'evento non replicati`)
        return false
      }
      inAttesa += da.length
      coda.push({ shas: da, lavoro })
      avanti()
      return true
    },
    /** File in attesa (in coda o nei lavori in corso). */
    get inAttesa() { return inAttesa },
    /** Eventi in lavorazione in questo momento. */
    get attivi() { return attivi },
    ferma() { fermo = true; coda.length = 0; inAttesa = 0 },
  }
}

module.exports = { creaCoda, INSIEME, MASSIMO }
