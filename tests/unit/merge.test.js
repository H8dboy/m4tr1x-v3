// @ts-check
'use strict'
/** tools/merge.js: il cancello del merge su main (22/10/2026), finché GitHub non ha check obbligatori. */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const { jobsDiCi, obbligatori, decidi } = require('../../tools/merge.js')

const YML = fs.readFileSync(path.join(__dirname, '..', '..', '.github', 'workflows', 'ci.yml'), 'utf8')
const PR = { state: 'OPEN', isDraft: false, baseRefName: 'main' }
const ok = (/** @type {string} */ name, conclusion = 'success', started_at = '2026-10-22T06:00:00Z', slug = 'github-actions') =>
  ({ name, status: 'completed', conclusion, started_at, app: { slug } })

test('obbligatori: tutti i job di ci.yml tranne release-gate, i01-rete sempre', () => {
  const jobs = jobsDiCi(YML)
  assert.ok(jobs.includes('i01-rete') && jobs.includes('tor') && jobs.includes('release-gate'), jobs.join(','))
  const r = obbligatori(YML)
  assert.ok(!r.includes('release-gate'))
  for (const j of jobs.filter(j => j !== 'release-gate')) assert.ok(r.includes(j), j)
  // tolto da ci.yml, resta obbligatorio
  assert.ok(obbligatori(YML.replace(/\n {2}i01-rete:\n/, '\n  altro:\n')).includes('i01-rete'))
})

test('decidi: verde solo con ogni job obbligatorio riuscito sull\'ultimo tentativo', () => {
  const req = obbligatori(YML)
  const tutti = req.map(n => ok(n))
  assert.deepEqual(decidi(PR, tutti, req), { ok: true, motivi: [] })

  const senza = (/** @type {string} */ n) => tutti.filter(c => c.name !== n)
  assert.match(decidi(PR, senza('i01-rete'), req).motivi.join(), /i01-rete: nessun esito/)
  assert.match(decidi(PR, [...senza('i01-rete'), ok('i01-rete', 'skipped')], req).motivi.join(), /i01-rete: skipped/)
  assert.match(decidi(PR, [...senza('tor'), ok('tor', 'neutral')], req).motivi.join(), /tor: neutral/)
  assert.match(decidi(PR, [...senza('test'), { ...ok('test'), status: 'in_progress', conclusion: null }], req).motivi.join(), /test: ancora in_progress/)
  // l'ultimo tentativo conta: verde vecchio + rosso nuovo = rosso
  assert.match(decidi(PR, [...tutti, ok('tor', 'failure', '2026-10-22T07:00:00Z')], req).motivi.join(), /tor: failure/)
  assert.equal(decidi(PR, [...senza('tor'), ok('tor', 'failure', '2026-10-22T05:00:00Z'), ok('tor')], req).ok, true)
  // un esito con lo stesso nome da un'altra app non vale
  assert.match(decidi(PR, [...senza('i01-rete'), ok('i01-rete', 'success', undefined, 'altra-app')], req).motivi.join(), /i01-rete: nessun esito/)
})

test('decidi: PR in bozza, chiusa, verso un altro ramo, o i01-rete fuori elenco = rifiuto', () => {
  const req = obbligatori(YML)
  const tutti = req.map(n => ok(n))
  assert.match(decidi({ ...PR, isDraft: true }, tutti, req).motivi.join(), /bozza/)
  assert.match(decidi({ ...PR, state: 'MERGED' }, tutti, req).motivi.join(), /non aperta/)
  assert.match(decidi({ ...PR, baseRefName: 'dev' }, tutti, req).motivi.join(), /verso dev/)
  assert.match(decidi(PR, tutti, req.filter(n => n !== 'i01-rete')).motivi.join(), /i01-rete non è tra/)
})
