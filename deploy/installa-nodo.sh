#!/usr/bin/env bash
# Installa (o aggiorna) il nodo M4TR1X v3 su Linux x86_64 con systemd. Va lanciato da root, dalla
# cartella del repo copiata sul server:   sudo bash deploy/installa-nodo.sh
#
# Cosa fa, e solo questo:
#  - utente di sistema `m4tr1x` senza shell e senza privilegi;
#  - Node FISSATO (versione di .node-version) in /opt/m4tr1x-node22, hash controllato;
#  - tor dal bundle del lock (app/tor-linux.lock.json) in /opt/m4tr1x-tor, hash del bundle, di tor e di ogni libreria;
#  - codice in /opt/m4tr1x (di root, non scrivibile dal nodo), dipendenze con `npm ci` senza script;
#  - configurazione in /etc/m4tr1x (node.json + passphrase generata, leggibili solo dal nodo), dati in /var/lib/m4tr1x;
#  - servizio systemd chiuso (deploy/m4tr1x-node.service). Nessuna porta aperta: il nodo ascolta su 127.0.0.1
#    e si raggiunge solo dal suo onion.
# Non tocca firewall, SSH, altri servizi. Rilanciarlo aggiorna codice e binari e tiene dati, chiavi e onion.
set -euo pipefail
umask 077

[ "$(id -u)" = 0 ] || { echo "serve root (sudo)"; exit 1; }
[ "$(uname -m)" = x86_64 ] || { echo "solo x86_64 per ora (lock di tor e Node per questa architettura)"; exit 1; }
command -v systemctl >/dev/null || { echo "serve systemd"; exit 1; }

SORGENTE="$(cd "$(dirname "$0")/.." && pwd)"
[ -f "$SORGENTE/node/bin/m4tr1x-node.js" ] || { echo "lanciare dalla cartella del repo"; exit 1; }

NODE_VERSIONE="$(tr -d '[:space:]' < "$SORGENTE/.node-version")"
# SHA-256 di node-v22.23.3-linux-x64.tar.xz da SHASUMS256.txt, firma GPG verificata l'11/11/2026
# (chiave 5BE8A3F6C8A5C01D106C0AD820B1A390B168D356, nell'elenco dei rilasci del README di nodejs/node).
# Cambiare versione = cambiare .node-version E questo hash, in review.
NODE_SHA256_22_23_3="df450af89261115ef9f9e3830c3eeb2cc9213b63c720b1af623cb5dcbe2e02de"
[ "$NODE_VERSIONE" = "22.23.3" ] || { echo ".node-version ($NODE_VERSIONE) senza hash verificato in questo script"; exit 1; }

NODE_DIR=/opt/m4tr1x-node22
TOR_DIR=/opt/m4tr1x-tor
CODICE=/opt/m4tr1x
CONF=/etc/m4tr1x
DATI=/var/lib/m4tr1x
LAVORO="$(mktemp -d)"
trap 'rm -rf "$LAVORO"' EXIT

scarica() { curl -fsSL --proto '=https' --tlsv1.2 --retry 3 -o "$2" "$1"; }
controlla() { echo "$1  $2" | sha256sum -c --quiet - || { echo "HASH SBAGLIATO: $2"; exit 1; }; }

echo "1/6 utente di sistema"
id m4tr1x >/dev/null 2>&1 || useradd --system --home-dir "$DATI" --no-create-home --shell /sbin/nologin m4tr1x

echo "2/6 Node $NODE_VERSIONE (hash fissato)"
if [ "$("$NODE_DIR/bin/node" --version 2>/dev/null)" != "v$NODE_VERSIONE" ]; then
  scarica "https://nodejs.org/dist/v$NODE_VERSIONE/node-v$NODE_VERSIONE-linux-x64.tar.xz" "$LAVORO/node.tar.xz"
  controlla "$NODE_SHA256_22_23_3" "$LAVORO/node.tar.xz"
  rm -rf "$NODE_DIR.nuovo" && mkdir -p "$NODE_DIR.nuovo"
  tar -xJf "$LAVORO/node.tar.xz" -C "$NODE_DIR.nuovo" --strip-components=1 --no-same-owner
  rm -rf "$NODE_DIR" && mv "$NODE_DIR.nuovo" "$NODE_DIR"
fi
chmod -R go-w "$NODE_DIR" && chmod 755 "$NODE_DIR"
NODE="$NODE_DIR/bin/node"

echo "3/6 tor dal lock (bundle, tor e librerie con hash)"
LOCK="$SORGENTE/app/tor-linux.lock.json"
leggi() { "$NODE" -e 'const l=require(process.argv[1]);const v=process.argv[2].split(".").reduce((o,k)=>o[k],l);process.stdout.write(typeof v==="string"?v:JSON.stringify(v))' "$LOCK" "$1"; }
TOR_SHA="$(leggi tor_sha256)"
if ! { [ -x "$TOR_DIR/tor" ] && echo "$TOR_SHA  $TOR_DIR/tor" | sha256sum -c --quiet - 2>/dev/null; }; then
  scarica "$(leggi bundle_url)" "$LAVORO/tor.tar.gz"
  controlla "$(leggi bundle_sha256)" "$LAVORO/tor.tar.gz"
  mkdir -p "$LAVORO/tor" && tar -xzf "$LAVORO/tor.tar.gz" -C "$LAVORO/tor" --no-same-owner
  rm -rf "$TOR_DIR.nuovo" && mkdir -p "$TOR_DIR.nuovo"
  cp "$LAVORO/tor/$(leggi tor_path_in_bundle)" "$TOR_DIR.nuovo/tor"
  controlla "$TOR_SHA" "$TOR_DIR.nuovo/tor"
  for lib in $("$NODE" -e 'console.log(Object.keys(require(process.argv[1]).librerie).join(" "))' "$LOCK"); do
    cp "$LAVORO/tor/tor/$lib" "$TOR_DIR.nuovo/$lib"
    controlla "$("$NODE" -e 'console.log(require(process.argv[1]).librerie[process.argv[2]])' "$LOCK" "$lib")" "$TOR_DIR.nuovo/$lib"
  done
  rm -rf "$TOR_DIR" && mv "$TOR_DIR.nuovo" "$TOR_DIR"
fi
chown -R root:root "$TOR_DIR" && chmod 755 "$TOR_DIR" "$TOR_DIR/tor" && chmod 644 "$TOR_DIR"/*.so*
LD_LIBRARY_PATH="$TOR_DIR" "$TOR_DIR/tor" --version | head -1

echo "4/6 codice e dipendenze"
rm -rf "$CODICE.nuovo" && mkdir -p "$CODICE.nuovo"
for voce in .node-version package.json package-lock.json node packages; do cp -a "$SORGENTE/$voce" "$CODICE.nuovo/"; done
( cd "$CODICE.nuovo" && PATH="$NODE_DIR/bin:$PATH" npm ci --omit=dev --ignore-scripts --no-audit --no-fund --loglevel=error )
chown -R root:root "$CODICE.nuovo" && chmod -R go-w,a+rX "$CODICE.nuovo"
rm -rf "$CODICE.vecchio" && { [ -d "$CODICE" ] && mv "$CODICE" "$CODICE.vecchio" || true; } && mv "$CODICE.nuovo" "$CODICE"

echo "5/6 configurazione e dati"
install -d -o root -g m4tr1x -m 0750 "$CONF"
install -d -o m4tr1x -g m4tr1x -m 0700 "$DATI"
if [ ! -s "$CONF/passphrase" ]; then
  head -c 48 /dev/urandom | base64 | tr -d '\n=+/' > "$CONF/passphrase"
  echo "   passphrase del vault generata in $CONF/passphrase (copiala in un posto sicuro: senza, le chiavi del nodo non si aprono)"
fi
chown m4tr1x:m4tr1x "$CONF/passphrase" && chmod 0600 "$CONF/passphrase"
if [ ! -f "$CONF/node.json" ]; then
  cat > "$CONF/node.json" <<JSON
{
  "dataDir": "$DATI",
  "keyPassphraseFile": "$CONF/passphrase",
  "network": "m4tr1x-test",
  "role": "user",
  "tor": true,
  "torBin": "$TOR_DIR/tor"
}
JSON
fi
chown root:m4tr1x "$CONF/node.json" && chmod 0640 "$CONF/node.json"

echo "6/6 servizio"
install -o root -g root -m 0644 "$SORGENTE/deploy/m4tr1x-node.service" /etc/systemd/system/m4tr1x-node.service
systemctl daemon-reload
systemctl enable m4tr1x-node >/dev/null
systemctl restart m4tr1x-node
echo "fatto. Stato: systemctl status m4tr1x-node · registro: journalctl -u m4tr1x-node -f"
echo "l'indirizzo onion compare in $DATI/onion (solo root e il nodo lo leggono) quando tor l'ha pubblicato"
