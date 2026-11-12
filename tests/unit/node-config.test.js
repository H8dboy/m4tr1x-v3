// @ts-check
'use strict'
/**
 * Configurazione del nodo: un blob è al massimo un PEZZO (ADR-0005, 4 MiB, la stessa costante dell'app). Un file più
 * grande arriva a pezzi, ognuno un blob: un nodo piccolo non riceve mai un corpo più grande, e un caricamento
 * interrotto riprende dai pezzi mancanti. Chi vuole di più lo scrive nel file di configurazione, in review.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const { normalize, PEZZO_BYTES } = require('../../node/src/config')

const ROOT = path.resolve(__dirname, '..', '..')

test('il blob massimo predefinito è un pezzo (4 MiB), come nell\'app', () => {
  assert.equal(PEZZO_BYTES, 4 * 1024 * 1024)
  const cfg = normalize({ dataDir: '/tmp/x', network: 'm4tr1x-test' })
  assert.equal(cfg.maxBlobBytes, PEZZO_BYTES)
  assert.equal(normalize({ dataDir: '/tmp/x', network: 'm4tr1x-test', maxBlobBytes: 1024 }).maxBlobBytes, 1024, 'si può scrivere nel file')
  const protocollo = fs.readFileSync(path.join(ROOT, 'app', 'protocol', 'src', 'lib.rs'), 'utf8')
  assert.match(protocollo, /pub const PEZZO: u64 = 4 \* 1024 \* 1024;/, 'la costante dell\'app non è più 4 MiB: cambiarle insieme')
})
