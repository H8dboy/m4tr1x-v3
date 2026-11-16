# M4TR1X v3

Social federato e privacy-first per la diaspora e le comunità censurate. Tutto passa da Tor.

**Stato:** repo pubblico dal 16/11/2026. Licenza AGPL-3.0-only (file `LICENSE`): il codec a celle vive anche in [m4tr1x-hive](https://github.com/H8dboy/m4tr1x-hive).

- Progetto: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- Cosa proteggiamo e cosa non promettiamo: [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md)
- Regole di sicurezza e i loro test: [`docs/SECURITY_INVARIANTS.md`](docs/SECURITY_INVARIANTS.md)
- Chiavi: [`docs/KEYS.md`](docs/KEYS.md) · Decisioni: [`docs/decisions/`](docs/decisions/)
- Piano a fasi: [`docs/MIGRATION_PLAN.md`](docs/MIGRATION_PLAN.md)

## Comandi

```
npm test                 # tutte le invarianti attive (+ elenco di quelle in attesa)
npm run check:secrets    # nessun segreto nei file tracciati
```

## Regole

- Mai segreti nei commit: chiavi, `.env`, database, chiavi onion, IP, email personali.
- Ogni regola di sicurezza ha un test che fallisce se viene violata.
- Nessuna release senza `release.json` firmato offline.
