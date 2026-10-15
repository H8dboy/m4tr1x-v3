// Prototipo F4a — interfaccia minima. Tutta la rete è nel lato Rust (Arti).
'use strict'
const { invoke } = window.__TAURI__.core
const $ = id => document.getElementById(id)
const log = msg => { const t = new Date().toLocaleTimeString(); $('log').textContent = `[${t}] ${msg}\n` + $('log').textContent }
const base = () => 'http://' + $('onion').value.trim().replace(/^http:\/\//, '').replace(/\/.*$/, '')
const results = []
const busy = on => document.querySelectorAll('button').forEach(b => { if (b.id !== 'b-copy') b.disabled = on })

try { const v = localStorage.getItem('f4a-onion'); if (v) $('onion').value = v } catch {}
$('onion').addEventListener('change', () => { try { localStorage.setItem('f4a-onion', $('onion').value.trim()) } catch {} })

async function run(label, fn) {
  busy(true)
  try { const r = await fn(); results.push({ label, ok: true, ...r }); return r }
  catch (e) { results.push({ label, ok: false, error: String(e) }); log(`${label}: ERRORE ${e}`); return null }
  finally { busy(false) }
}

$('b-boot').onclick = () => run('bootstrap', async () => {
  log('avvio Tor (Arti)…')
  const r = await invoke('bootstrap')
  log(`Tor pronto in ${(r.ms / 1000).toFixed(1)} s${r.already ? ' (già avviato)' : ''}`)
  return r
})

$('b-health').onclick = () => run('health', async () => {
  const r = await invoke('fetch', { url: base() + '/health' })
  log(`collegamento: ${r.status} · connessione ${(r.connect_ms / 1000).toFixed(1)} s · totale ${(r.total_ms / 1000).toFixed(1)} s`)
  return r
})

$('b-10m').onclick = () => run('blob10m', async () => {
  log('scarico 10 MB…')
  const r = await invoke('fetch', { url: base() + '/blob10m' })
  log(`10 MB: ${(r.body_bytes / 1048576).toFixed(1)} MB in ${(r.total_ms / 1000).toFixed(1)} s · ${r.mb_per_s.toFixed(2)} MB/s · hash ${r.sha256_ok ? 'OK' : 'DIVERSO'}`)
  return r
})

$('b-10x').onclick = async () => {
  let ok = 0
  for (let i = 1; i <= 10; i++) {
    const t0 = Date.now()
    const r = await run(`connect-${i}`, () => invoke('fetch', { url: base() + '/health' }))
    const s = ((Date.now() - t0) / 1000).toFixed(1)
    if (r && Date.now() - t0 <= 60000) ok++
    log(`collegamento ${i}/10: ${r ? 'ok' : 'fallito'} in ${s} s`)
  }
  log(`RISULTATO: ${ok}/10 collegamenti entro 60 s (criterio: almeno 9)`)
}

$('b-soak').onclick = async () => {
  const end = Date.now() + 30 * 60 * 1000
  let ok = 0, fail = 0
  log('prova di 30 minuti: un collegamento ogni 20 s…')
  while (Date.now() < end) {
    const r = await run('soak', () => invoke('fetch', { url: base() + '/health' }))
    r ? ok++ : fail++
    if ((ok + fail) % 9 === 0) log(`… ${ok} ok, ${fail} falliti`)
    await new Promise(res => setTimeout(res, 20000))
  }
  log(`30 MINUTI FINITI: ${ok} ok, ${fail} falliti`)
}

$('b-copy').onclick = async () => {
  const text = JSON.stringify({ ua: navigator.userAgent, results }, null, 1)
  try { await navigator.clipboard.writeText(text); log('risultati copiati negli appunti') }
  catch { log('copia non riuscita: risultati qui sotto'); log(text) }
}
