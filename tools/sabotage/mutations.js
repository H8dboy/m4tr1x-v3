// @ts-check
'use strict'
/**
 * Prove di sabotaggio: per ogni regola di sicurezza, una o più modifiche che la
 * ROMPONO. Il test della regola deve fallire con la modifica applicata.
 * Una modifica che non si applica (il codice è cambiato) è un errore della prova:
 * va aggiornata qui, non ignorata.
 *
 * Campi: id (unico), rule (I-xx), name, file, test (file di test della regola) e
 * le modifiche: `find`/`replace` (una) oppure `edits: [{find, replace}, …]` (più
 * modifiche nello stesso file). Ogni `find` deve comparire una sola volta.
 */
const T = (/** @type {string} */ n) => `tests/invariants/${n}.test.js`

/** @type {Array<{ id: string, rule: string, name: string, file: string, test: string, find?: string, replace?: string, edits?: Array<{ find: string, replace: string }> }>} */
module.exports = [
  // ─── F0 ───────────────────────────────────────────────────────────────────
  { id: 'i23-rete', rule: 'I-23', name: 'rete della firma non controllata', file: 'tools/release/verify-release.js',
    find: "  if (sig.network !== network) throw new Error(`firma per la rete \"${sig.network}\", attesa \"${network}\"`)\n  if (!protocol.verifyDomain(pubHex, String(sig.sig), DOMAIN, network, manifestBytes))",
    replace: "  if (!protocol.verifyDomain(pubHex, String(sig.sig), DOMAIN, sig.network, manifestBytes))", test: T('inv-23-release-gate') },
  { id: 'i23-hash', rule: 'I-23', name: 'hash dei pacchetti non controllato', file: 'tools/release/verify-release.js',
    find: "    if (actual !== String(sha).toLowerCase()) throw new Error(`hash diverso per ${name}`)", replace: '', test: T('inv-23-release-gate') },
  { id: 'i25-action-tag', rule: 'I-25', name: 'action fissata per tag invece che per hash', file: '.github/workflows/ci.yml',
    // il checkout con fetch-depth è l'unico: la prova vuole un testo che compaia una volta sola
    find: 'actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1\n        with:\n          persist-credentials: false\n          fetch-depth: 0',
    replace: 'actions/checkout@v7\n        with:\n          persist-credentials: false\n          fetch-depth: 0', test: T('inv-25-deps') },
  { id: 'i25-banned', rule: 'I-25', name: 'pacchetto vietato e non approvato tra le dipendenze', file: 'package.json',
    find: '  "dependencies": {\n', replace: '  "dependencies": {\n    "axios": "1.7.9",\n', test: T('inv-25-deps') },
  { id: 'i28-doc', rule: 'I-28', name: 'IP vero ammesso se accanto a uno di documentazione', file: 'tools/security/scan-secrets.js',
    find: "    const real = name === 'IP pubblico' ? hits.filter(ip => !DOC_IP_PREFIXES.some(d => ip.startsWith(d))) : hits",
    replace: "    const real = name === 'IP pubblico' && hits.some(ip => DOC_IP_PREFIXES.some(d => ip.startsWith(d))) ? [] : hits", test: T('inv-28-secrets') },
  { id: 'i28-nsec', rule: 'I-28', name: 'chiavi Nostr private non cercate', file: 'tools/security/scan-secrets.js',
    find: "  ['chiave privata Nostr (nsec)', /\\bnsec1[02-9ac-hj-np-z]{58}\\b/],\n", replace: '', test: T('inv-28-secrets') },

  // ─── F1 ───────────────────────────────────────────────────────────────────
  { id: 'i06-json', rule: 'I-06', name: 'segreto nel JSON della chiave', file: 'packages/keys/index.js',
    find: '  toJSON() { return { role: this.role, version: this.version, publicKey: this.publicKey } }',
    replace: "  toJSON() { return { role: this.role, s: Buffer.from(SECRETS.get(this)).toString('hex') } }", test: T('inv-06-keys-at-rest') },
  { id: 'i06-scrypt', rule: 'I-06', name: 'scrypt sotto il minimo (N=2^14)', file: 'packages/keys/index.js',
    find: 'const SCRYPT_MIN = Object.freeze({ N: 2 ** 17, r: 8, p: 1 })', replace: 'const SCRYPT_MIN = Object.freeze({ N: 2 ** 14, r: 8, p: 1 })', test: T('inv-06-keys-at-rest') },
  // dopo ADR-0017 la famiglia blocca già identità→spesa: il permesso conta DENTRO la stessa famiglia
  { id: 'i07-perm', rule: 'I-07', name: 'l\'identità può firmare una revoca (stessa famiglia)', file: 'packages/keys/index.js',
    find: "  identity: Object.freeze(['nostr-event', 'identity/migrate', 'dm/announce']),",
    replace: "  identity: Object.freeze(['nostr-event', 'identity/migrate', 'dm/announce', 'identity/revoke']),", test: T('inv-07-role-separation') },
  { id: 'i07-release', rule: 'I-07', name: 'la chiave di release può autorizzare spese (stessa famiglia)', file: 'packages/keys/index.js',
    find: "  release: Object.freeze(['release/manifest']),",
    replace: "  release: Object.freeze(['release/manifest', 'h8/spend']),", test: T('inv-07-role-separation') },
  { id: 'i08-rete', rule: 'I-08', name: 'rete fuori dal digest firmato', file: 'packages/protocol/index.js',
    find: 'const head = new TextEncoder().encode(`m4tr1x/${domain}/v1\\n${network}\\n`)', replace: 'const head = new TextEncoder().encode(`m4tr1x/${domain}/v1\\n`)', test: T('inv-08-domain-sep') },
  { id: 'i15-off', rule: 'I-15', name: 'controllo dei dati H8 negli eventi spento', file: 'packages/protocol/index.js',
    find: 'function h8Violation(ev) {', replace: 'function h8Violation(ev) { return null', test: T('inv-15-no-h8-in-events') },
  { id: 'i40-wallet', rule: 'I-40', name: 'wallet riportato a Schnorr', file: 'packages/keys/index.js',
    find: "  wallet: 'ml-dsa-65', 'issuer-root'", replace: "  wallet: 'schnorr', 'issuer-root'", test: T('inv-40-signature-families') },
  { id: 'i40-anyfam', rule: 'I-40', name: 'verifica accetta qualsiasi famiglia', file: 'packages/protocol/index.js',
    find: '    return keys.verifyDigest(/** @type {any} */ (fam), pubkeyHex, sigHex, domainDigest(domain, network, payload))',
    replace: "    const d = domainDigest(domain, network, payload)\n    return keys.verifyDigest('schnorr', pubkeyHex, sigHex, d) || keys.verifyDigest('ml-dsa-65', pubkeyHex, sigHex, d)", test: T('inv-40-signature-families') },
  { id: 'i40-release', rule: 'I-40', name: 'release verificata con Schnorr', file: 'packages/protocol/index.js',
    find: "'release/manifest': 'ml-dsa-65',", replace: "'release/manifest': 'schnorr',", test: T('inv-23-release-gate') },

  // ─── F2 ───────────────────────────────────────────────────────────────────
  { id: 'i02-bind', rule: 'I-02', name: 'ascolto su tutte le interfacce di default', file: 'node/src/config.js',
    find: "const host = c.host || '127.0.0.1'", replace: "const host = c.host || '0.0.0.0'; c.allowExternalBind = true", test: T('inv-02-bind') },
  { id: 'i03-host', rule: 'I-03', name: 'Host non controllato', file: 'node/src/http.js',
    find: '    const name = hostName(req.headers.host)\n    if (!name) return false', replace: '    return true\n    const name = hostName(req.headers.host)\n    if (!name) return false', test: T('inv-03-host') },
  { id: 'i04-star', rule: 'I-04', name: 'CORS con *', file: 'node/src/http.js',
    find: "    if (typeof origin === 'string' && cfg.allowedOrigins.includes(origin)) {\n      res.setHeader('Access-Control-Allow-Origin', origin)",
    replace: "    if (typeof origin === 'string') {\n      res.setHeader('Access-Control-Allow-Origin', '*')", test: T('inv-04-cors') },
  { id: 'i17-upload', rule: 'I-17', name: 'upload senza controllo dell\'hash', file: 'node/src/blobs.js',
    find: '        if (sha !== auth.x) return fail(', replace: '        if (false) return fail(', test: T('inv-17-blob-hash') },
  { id: 'i17-peer', rule: 'I-17', name: 'lettura da peer senza controllo dell\'hash', file: 'node/src/blobs.js',
    find: "        if (h.digest('hex') !== sha) return reject(", replace: '        if (false) return reject(', test: T('inv-17-blob-hash') },
  { id: 'i18-ip', rule: 'I-18', name: 'IP del client nel log', file: 'node/src/log.js',
    find: '  return (/** @type {string} */ msg) => out(scrub(msg))', replace: "  return (/** @type {string} */ msg) => out(msg + ' da 127.0.0.1')", test: T('inv-18-no-ip-logs') },
  { id: 'i26-crash', rule: 'I-26', name: 'messaggi malformati senza nessuna protezione (interna ed esterna)', file: 'node/src/relay.js', test: T('inv-26-fuzz'),
    edits: [
      { find: "    ws.on('message', raw => {\n      try {\n        let msg\n        try { msg = JSON.parse(raw.toString()) } catch { return send(['NOTICE', 'invalid: non è JSON']) }\n        if (!Array.isArray(msg) || typeof msg[0] !== 'string') return send(['NOTICE', 'invalid: atteso [tipo, ...]'])",
        replace: "    ws.on('message', raw => {\n      {\n        let msg\n        msg = JSON.parse(raw.toString())" },
      { find: "      } catch (e) { log(`relay: errore nel messaggio (${/** @type {Error} */ (e).message})`) }\n    })",
        replace: "      }\n    })" },
    ] },
  { id: 'i30-onion', rule: 'I-30', name: 'onion senza chiave admin', file: 'node/src/http.js',
    find: "      if (!isAdmin(req)) return json(res, 401, { error: 'chiave admin richiesta' })", replace: "      if (false) return json(res, 401, { error: 'chiave admin richiesta' })", test: T('inv-30-33-admin-onion') },
  { id: 'i33-public', rule: 'I-33', name: 'admin anche sul listener dell\'onion', file: 'node/src/http.js',
    find: "      if (isPublic) return json(res, 404, { error: 'non trovato' })\n      if (!isAdmin(req))", replace: '      if (!isAdmin(req))', test: T('inv-30-33-admin-onion') },

  // ─── F3 ───────────────────────────────────────────────────────────────────
  { id: 'i05-peer', rule: 'I-05', name: 'parser dei peer accetta indirizzi in chiaro', file: 'node/src/peers.js',
    find: "  if (u.protocol !== 'http:' || (!isOnionV3(u.hostname) && !loop)) return null", replace: "  if (u.protocol !== 'http:') return null", test: T('inv-05-onion-only') },
  { id: 'i05-anchor', rule: 'I-05', name: 'parser delle ancore accetta indirizzi in chiaro', file: 'node/src/directory.js',
    find: "  if (u.protocol !== 'http:' || (!isOnionV3(u.hostname) && !loop)) return null", replace: "  if (u.protocol !== 'http:') return null", test: T('inv-05-onion-only') },
  { id: 'i29-ip', rule: 'I-29', name: 'IP nella scheda accettato', file: 'packages/protocol/index.js',
    find: "  if (ANY_IPV4.test(haystack) || ANY_IPV6.test(haystack)) return no('indirizzo IP nella scheda')", replace: '', test: T('inv-29-descriptor') },
  { id: 'i29-ttl', rule: 'I-29', name: 'scadenza oltre 7 giorni accettata', file: 'packages/protocol/index.js',
    find: "  if (expAt - ev.created_at > MAX_DESCRIPTOR_TTL) return no('scadenza oltre 7 giorni')", replace: '', test: T('inv-29-descriptor') },
  { id: 'i29-nodepost', rule: 'I-29', name: 'la chiave del nodo firma post', file: 'packages/protocol/index.js',
    find: "  if (identityKey.role === keys.NODE_ROLE && template.kind !== DESCRIPTOR_KIND)\n    throw new Error('la chiave del nodo firma solo schede di directory')", replace: '', test: T('inv-29-descriptor') },
  { id: 'i38-query', rule: 'I-38', name: 'scheda scaduta ancora restituita', file: 'node/src/relay.js',
    find: "    where.push('(e.expires_at IS NULL OR e.expires_at > ?)'); params.push(nowSec())", replace: '', test: T('inv-38-expiry') },
  { id: 'i38-purge', rule: 'I-38', name: 'scheda scaduta mai cancellata', file: 'node/src/relay.js',
    find: '  function purgeExpired() { const n = Number(purgeStmt.run(nowSec()).changes); totals = /** @type {any} */ (totalUse.get()); return n }', replace: '  function purgeExpired() { return 0 }', test: T('inv-38-expiry') },
  { id: 'i38-replace', rule: 'I-38', name: 'il rinnovo non sostituisce la scheda', file: 'node/src/relay.js',
    find: '    const repl = isReplaceable(ev.kind) || isParamReplaceable(ev.kind)', replace: '    const repl = false', test: T('inv-38-expiry') },
  { id: 'i38-renew', rule: 'I-38', name: 'il nodo non rinnova', file: 'node/src/directory.js',
    find: '    timer = setTimeout(loop, (ok ? cfg.descriptorRenewSec : Math.min(60, cfg.descriptorRenewSec)) * 1000)', replace: '', test: T('inv-38-expiry') },
  { id: 'i39-perkey', rule: 'I-39', name: 'nessun limite per chiave', file: 'node/src/directory.js',
    find: "        if (now - known.last_update < L.minUpdateIntervalSec) return [false, 'rate-limited: aggiornamento troppo frequente per questa chiave']", replace: '', test: T('inv-39-anchor-limits') },
  { id: 'i39-fixedpow', rule: 'I-39', name: 'difficoltà fissa (niente adattamento)', file: 'node/src/directory.js',
    find: '  return Math.min(L.powBitsMax, L.powBitsNewKey + extra)', replace: '  return L.powBitsNewKey', test: T('inv-39-anchor-limits') },
  { id: 'i39-noretry', rule: 'I-39', name: 'il nodo non rilegge la difficoltà', file: 'node/src/directory.js',
    find: '    if (need > 0 && need <= POW_CAP) {', replace: '    if (false) {', test: T('inv-39-anchor-limits') },
  { id: 'i39-cap', rule: 'I-39', name: 'nessun tetto di schede attive', file: 'node/src/directory.js',
    find: "      return [false, 'blocked: directory piena']", replace: '      {}', test: T('inv-39-anchor-limits') },
  { id: 'i39-nopow', rule: 'I-39', name: 'nessuna prova di lavoro', file: 'node/src/directory.js',
    find: '        if (need > 0 && protocol.leadingZeroBits(ev.id) < need)', replace: '        if (false)', test: T('inv-39-anchor-limits') },
  // ─── Debolezze 3–5 chiuse prima di F4 ───────────────────────────────────────
  { id: 'i06-nopass', rule: 'I-06', name: 'il nodo parte con una passphrase di default', file: 'node/src/node.js',
    find: "  throw new Error('passphrase delle chiavi del nodo mancante: keyPassphrase, keyPassphraseFile o M4TR1X_KEY_PASSPHRASE')",
    replace: "  return 'default'", test: T('inv-06b-node-secrets') },
  { id: 'i06-plainleft', rule: 'I-06', name: 'i file in chiaro vecchi restano sul disco', file: 'node/src/node.js',
    find: "    fs.rmSync(f, { force: true })", replace: '', test: T('inv-06b-node-secrets') },
  { id: 'i41-size', rule: 'I-41', name: 'nessun limite di dimensione per evento', file: 'node/src/relay.js',
    find: "    if (size > L.maxEventBytes) return [false, `invalid: evento troppo grande (${size} byte, massimo ${L.maxEventBytes})`]", replace: '', test: T('inv-41-relay-space') },
  { id: 'i41-author', rule: 'I-41', name: 'nessuna quota per autore', file: 'node/src/relay.js',
    find: "      if (a.n >= L.perAuthorMaxEvents || a.b + size > L.perAuthorMaxBytes) return [false, 'blocked: quota di questo autore esaurita su questo nodo']", replace: '', test: T('inv-41-relay-space') },
  { id: 'i41-total', rule: 'I-41', name: 'nessun tetto globale', file: 'node/src/relay.js',
    find: "      if (totals.n >= L.maxTotalEvents || totals.b + size > L.maxTotalBytes) return [false, 'blocked: relay pieno']", replace: '', test: T('inv-41-relay-space') },
  { id: 'i39-peerbudget', rule: 'I-39', name: 'nessun budget per ancora-peer', file: 'node/src/directory.js',
    find: "      if (recent.length >= L.peerNewKeysPerHour) { peerBudget.set(key, recent); return [false, 'rate-limited: budget di chiavi nuove di questa ancora-peer esaurito'] }", replace: '', test: T('inv-39-anchor-limits') },
]
