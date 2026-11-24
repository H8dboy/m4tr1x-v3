// @ts-check
'use strict'
/**
 * I-62 — I messaggi privati DENTRO l'app (m4tr1x-posta + guscio):
 *  (1) al nodo arrivano solo buste (kind 1059) e annunci (kind 10477): mai il testo, mai l'identità di chi scrive;
 *  (2) si cifra SOLO per la chiave DM dichiarata dall'annuncio valido dell'identità scelta: senza annuncio non
 *      parte niente, e l'annuncio di un altro (nodo malevolo) non devia il messaggio;
 *  (3) un mittente senza annuncio valido resta sconosciuto: nessuna identità, nessun nome;
 *  (4) la chiave DM si legge dal portachiavi solo dentro m4tr1x-posta: il guscio non la tocca e
 *      all'interfaccia arrivano solo testi e chiavi pubbliche (con I-31).
 *  (5) un annuncio vale solo con UNA scadenza (NIP-40) entro 90 giorni dalla sua data e fino a quella (l'app ne
 *      firma da 30 e li rinnova da sola sotto i 10): un nodo che ripropone un annuncio vecchio lo fa al massimo
 *      fino alla sua scadenza (24/11/2026, chiude il limite dichiarato);
 *  (6) la chiave DM RUOTA a ogni rinnovo per scadenza (~20 giorni): chiave nuova casuale, le DM_TENUTE più recenti
 *      restano per i messaggi recenti e sono dichiarate nell'annuncio, le più vecchie si cancellano; un'identità
 *      diversa non eredita le chiavi ruotate (24/11/2026, app/identity/tests/rotazione_dm.rs).
 * Parte dinamica: app/posta/tests/posta.rs (nodo finto, portachiavi in memoria). Parte statica: qui.
 */
const { test } = require('node:test')
const assert = require('node:assert/strict')
const fs = require('fs')
const path = require('path')
const { spawnSync } = require('child_process')

const ROOT = path.resolve(__dirname, '..', '..')
const APP = path.join(ROOT, 'app')
const read = (/** @type {string} */ f) => fs.readFileSync(f, 'utf8')
const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8', shell: process.platform === 'win32' })

test('la chiave dei messaggi vive solo in m4tr1x-posta: il guscio passa il portachiavi e riceve testi', () => {
  const lib = read(path.join(APP, 'shell', 'src', 'lib.rs')).replace(/\/\/[^\n]*/g, '')
  // il guscio non apre né incarta buste e non nomina il ruolo DM: lo fa solo m4tr1x-posta
  assert.ok(!/m4tr1x_dm\b/.test(lib), 'il guscio usa m4tr1x-dm direttamente')
  assert.ok(!/Ruolo::Dm/.test(lib), 'il guscio nomina la chiave DM')
  const toml = read(path.join(APP, 'shell', 'Cargo.toml'))
  assert.ok(!/^\s*m4tr1x-dm\s*=/m.test(toml), 'm4tr1x-dm tra le dipendenze dirette del guscio')
  // i due comandi della posta passano da m4tr1x-posta
  for (const [cmd, fun] of [['messaggi', 'm4tr1x_posta::leggi('], ['messaggio_invia', 'm4tr1x_posta::invia(']]) {
    const corpo = new RegExp(`async fn ${cmd}\\([\\s\\S]*?\\n}\\n`).exec(lib)
    assert.ok(corpo && corpo[0].includes(fun), `${cmd} non passa da ${fun}`)
  }
})

test('m4tr1x-posta: nessuna libreria di rete, chiave DM mai tenuta attraverso un\'attesa', () => {
  const toml = read(path.join(APP, 'posta', 'Cargo.toml'))
  const dipendenze = /\[dependencies\]([\s\S]*?)(?:\n\[|$)/.exec(toml)
  assert.ok(dipendenze)
  const nomi = [...dipendenze[1].matchAll(/^\s*([a-z0-9_-]+)\s*=/gm)].map(m => m[1]).sort()
  assert.deepEqual(nomi, ['m4tr1x-dm', 'm4tr1x-feed', 'm4tr1x-identity', 'm4tr1x-protocol', 'm4tr1x-transport', 'serde_json', 'thiserror'], `dipendenze di m4tr1x-posta: ${nomi}`)
  const src = read(path.join(APP, 'posta', 'src', 'lib.rs')).replace(/\/\/[^\n]*/g, '')
  // ogni lettura della chiave DM sta in una funzione NON async, oppure in un blocco che si chiude prima di ogni .await
  // (dm_pubbliche e ruota_dm tornano solo chiavi pubbliche: i segreti restano dentro m4tr1x-identity)
  assert.ok(!/carica\(p, Ruolo::Dm/.test(src), 'm4tr1x-posta legge una versione fissa della chiave DM invece della corrente')
  const letture = [...src.matchAll(/(chiavi_dm|chiave_dm_corrente)\(p\)/g)].length
  assert.equal(letture, 3, `letture della chiave DM attese: 3 (annuncio, apertura, incarto), trovate ${letture}`)
  const invia = /pub async fn invia[\s\S]*?\n}\n/.exec(src)
  assert.ok(invia)
  const blocco = /let buste = \{([\s\S]*?)\n    \};/.exec(invia[0])
  assert.ok(blocco && blocco[1].includes('chiave_dm_corrente(p)') && !blocco[1].includes('.await'), 'in invia la chiave DM attraversa un\'attesa di rete')
  assert.equal([...invia[0].matchAll(/(chiavi_dm|chiave_dm_corrente)\(p\)/g)].length, 1)
  // l'identità non si carica mai qui: firma solo tramite firma_come_identita
  assert.ok(!/Ruolo::Identity/.test(src), 'm4tr1x-posta carica la chiave d\'identità')
  assert.match(src, /annuncio_dm_con\(rete, &chi\.identity, \|d\| firma_come_identita\(p, d\)\.ok\(\)/)
})

test('posta (Rust): solo buste e annunci al nodo, destinatario dall\'annuncio giusto, sconosciuti senza nome', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(APP, 'Cargo.toml'), '-p', 'm4tr1x-posta', '--test', 'posta'],
    { cwd: APP, encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  for (const t of ['due_utenti_si_scrivono_e_al_nodo_arrivano_solo_buste_e_annunci', 'senza_annuncio_del_destinatario_non_parte_niente',
    'testo_o_destinatario_non_validi_si_fermano_prima_della_rete', 'mittente_senza_annuncio_resta_sconosciuto_e_le_buste_forgiate_non_si_mostrano',
    'un_annuncio_che_rivendica_la_chiave_di_un_altro_non_ruba_il_nome', 'un_nodo_che_risponde_con_l_annuncio_di_un_altro_non_fa_cifrare_per_la_chiave_sbagliata',
    // scadenza degli annunci (24/11/2026): rinnovo da solo, annunci senza scadenza o scaduti non valgono
    'l_annuncio_in_scadenza_si_rinnova_da_solo_quello_fresco_no', 'un_annuncio_senza_scadenza_di_prima_si_rifa_e_non_vale_per_chi_scrive', 'a_chi_ha_l_annuncio_scaduto_non_si_scrive',
    // rotazione della chiave DM (24/11/2026)
    'dopo_la_rotazione_i_messaggi_recenti_si_leggono_e_quelli_nuovi_vanno_alla_chiave_nuova'])
    assert.match(out, new RegExp(`${t} \\.\\.\\. ok`), `manca o fallisce la prova ${t}`)
})

test('rotazione della chiave DM (Rust, m4tr1x-identity): chiave nuova casuale, finestra di DM_TENUTE, identità diversa non eredita', { skip: cargo.status !== 0 ? 'cargo non disponibile' : false, timeout: 900000 }, () => {
  const r = spawnSync('cargo', ['test', '--locked', '--manifest-path', path.join(APP, 'Cargo.toml'), '-p', 'm4tr1x-identity', '--test', 'rotazione_dm'],
    { cwd: APP, encoding: 'utf8', timeout: 900000, maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' })
  const out = (r.stdout || '') + (r.stderr || '')
  assert.equal(r.status, 0, out.slice(-4000))
  for (const t of ['senza_rotazione_la_corrente_e_quella_del_seme', 'ogni_rotazione_da_una_chiave_nuova_e_tiene_solo_le_ultime',
    'recupero_della_stessa_identita_tiene_le_ruotate_un_altra_le_dimentica', 'voce_dm_corrente_storta_e_un_errore'])
    assert.match(out, new RegExp(`${t} \\.\\.\\. ok`), `manca o fallisce la prova ${t}`)
})
