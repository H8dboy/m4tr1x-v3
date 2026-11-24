#!/usr/bin/env bash
# I-01 dinamica, rete dell'utente dedicato dell'app (decisioni del fondatore, 21/10/2026, opzione A).
# Solo CI Linux, da root. L'app, WebKit, tauri-driver e il tor figlio dell'app girano come $UTENTE:
#   - loopback: ammesso, TRANNE i resolver locali (DNS 53, DNS su TLS 853, mDNS 5353, LLMNR 5355,
#     TCP e UDP) su TUTTO 127.0.0.0/8 e ::1;
#   - resolver locali via socket UNIX, che iptables non vede (varlink di systemd-resolved, bus di sistema
#     D-Bus con resolve1, avahi, nscd): negati al SOLO $UTENTE con un'ACL u:$UTENTE:--- sui socket. Non si
#     fermano i servizi: senza systemd-resolved resterebbe senza DNS anche l'agente del runner di GitHub;
#   - controllo POSITIVO prima della prova (rete.sh controllo): una risoluzione come $UTENTE deve fallire e il
#     contatore delle violazioni deve salire; poi si azzera e parte la prova vera;
#   - fuori dal loopback: SOLO le destinazioni dell'ipset m4relay (relay Tor, tools/i01/consentiti.js);
#     tutto il resto (UDP, IPv6, altri indirizzi) contato in M4VIOLA, registrato e rifiutato.
# Limite (scritto in I-01): un collegamento dell'app verso un indirizzo che è anche un relay non si vede.
#
# Uso: rete.sh prepara <file ipset>   |   rete.sh conta   |   rete.sh pulisci
set -euo pipefail
UTENTE=m4app
RESOLVER_PORTE=(53 853 5353 5355)
# passi di `prepara` su stderr con l'ora: se si blocca (visto in CI il 24/11/2026: 60 s senza nessuna riga), si sa dove
passo() { echo "passo $(date +%T.%3N): $*" >&2; }

case "${1:-}" in
  prepara)
    passo "utente $UTENTE: c'è già?"
    if ! timeout 20 id -u "$UTENTE" >/dev/null 2>&1; then
      passo "utente $UTENTE: useradd"
      # in CI (24/11/2026) questo passo è rimasto appeso 60 s: con un tetto, chi tiene i file degli utenti, e un secondo tentativo
      if ! timeout 20 useradd --create-home --shell /usr/sbin/nologin "$UTENTE"; then
        echo "useradd non riuscito o appeso; processi e blocchi:" >&2
        ps -eo pid,etime,cmd | grep -E 'useradd|passwd|shadow|dpkg|apt|nscd|userdb|sssd|cloud-init|waagent' | grep -v grep >&2 || true
        ls -la /etc/*.lock /etc/.pwd.lock 2>/dev/null >&2 || true
        id -u "$UTENTE" >/dev/null 2>&1 || timeout 30 useradd --create-home --shell /usr/sbin/nologin "$UTENTE"
      fi
    fi
    passo "ACL sui socket dei resolver"
    # resolver via socket UNIX: negati solo a $UTENTE (connect su un socket UNIX richiede la scrittura)
    for sock in /run/systemd/resolve/io.systemd.Resolve /run/systemd/resolve/io.systemd.Resolve.Monitor \
                /run/dbus/system_bus_socket /run/avahi-daemon/socket /run/nscd/socket; do
      if [ -S "$sock" ]; then setfacl -m "u:$UTENTE:---" "$sock"; echo "negato a $UTENTE: $sock"; fi
    done
    passo "ricerca di altri socket di resolver"
    # qualsiasi altro socket UNIX di resolver noto ancora raggiungibile da $UTENTE è un errore
    for sock in $(find /run /var/run -maxdepth 3 -type s \( -name 'io.systemd.Resolve*' -o -name 'system_bus_socket' -o -path '*avahi*' -o -path '*nscd*' \) 2>/dev/null); do
      if sudo -u "$UTENTE" test -w "$sock"; then echo "resolver locale raggiungibile da $UTENTE: $sock" >&2; exit 1; fi
    done

    passo "ipset"
    ipset create m4relay hash:ip,port -exist
    ipset flush m4relay
    ipset restore -exist < "$2"
    echo "ipset m4relay: $(ipset list m4relay | grep -c ',tcp:') destinazioni"

    passo "regole iptables"
    for T in iptables ip6tables; do
      $T -N M4VIOLA 2>/dev/null || $T -F M4VIOLA
      $T -A M4VIOLA -j LOG --log-prefix "M4-I01 " --log-uid
      $T -A M4VIOLA -j REJECT
      $T -N M4APP 2>/dev/null || $T -F M4APP
      for p in "${RESOLVER_PORTE[@]}"; do
        $T -A M4APP -o lo -p udp --dport "$p" -j M4VIOLA
        $T -A M4APP -o lo -p tcp --dport "$p" -j M4VIOLA
      done
      $T -A M4APP -o lo -j ACCEPT
      $T -C OUTPUT -m owner --uid-owner "$UTENTE" -j M4APP 2>/dev/null || $T -I OUTPUT 1 -m owner --uid-owner "$UTENTE" -j M4APP
    done
    # solo IPv4 verso i relay (tor del client non usa IPv6 di suo); IPv6 fuori dal loopback: tutto violazione
    iptables -A M4APP -p tcp -m set --match-set m4relay dst,dst -j ACCEPT
    iptables -A M4APP -j M4VIOLA
    ip6tables -A M4APP -j M4VIOLA
    passo "fatto"
    ;;

  controllo)
    # controllo POSITIVO: senza, contatori a 0 potrebbero voler dire "regole che non vedono niente"
    # ogni prova si conta DA SOLA: sommate, la risoluzione (sempre contata) nasconderebbe un host esterno ammesso
    viola() { iptables -nvxL M4VIOLA | awk 'NR==3 {print $1}'; }
    prima=$(viola)
    if sudo -u "$UTENTE" getent ahosts torproject.org >/dev/null 2>&1; then echo "controllo: $UTENTE risolve ancora i nomi" >&2; exit 1; fi
    dns=$(viola)
    if [ "$dns" -le "$prima" ]; then echo "controllo: la risoluzione non è stata contata ($prima → $dns)" >&2; exit 1; fi
    # indirizzo di documentazione (RFC 5737): il pacchetto esce dalle regole e si conta prima di andare ovunque
    if sudo -u "$UTENTE" timeout 10 bash -c 'exec 3<>/dev/tcp/192.0.2.1/443' 2>/dev/null; then echo "controllo: $UTENTE raggiunge un host fuori dai relay" >&2; exit 1; fi
    dopo=$(viola)
    if [ "$dopo" -le "$dns" ]; then echo "controllo: l'host fuori dai relay non è stato contato ($dns → $dopo)" >&2; exit 1; fi
    echo "controllo positivo: risoluzione ($prima → $dns) e host esterno ($dns → $dopo) bloccati e contati; contatori azzerati"
    iptables -Z M4VIOLA; ip6tables -Z M4VIOLA; iptables -Z M4APP; ip6tables -Z M4APP
    ;;

  conta)
    # pacchetti arrivati in M4VIOLA (prima regola: LOG) e verso i relay (prova che tor ha lavorato davvero)
    v4=$(iptables -nvxL M4VIOLA | awk 'NR==3 {print $1}')
    v6=$(ip6tables -nvxL M4VIOLA | awk 'NR==3 {print $1}')
    relay=$(iptables -nvxL M4APP | awk '/match-set m4relay/ {print $1}')
    lo=$(iptables -nvxL M4APP | awk '$7=="lo" && $3=="ACCEPT" {print $1}')
    echo "violazioni_ipv4=${v4:-?} violazioni_ipv6=${v6:-?} pacchetti_verso_relay=${relay:-?} pacchetti_loopback=${lo:-?}"
    ;;

  pulisci)
    for T in iptables ip6tables; do
      $T -D OUTPUT -m owner --uid-owner "$UTENTE" -j M4APP 2>/dev/null || true
      $T -F M4APP 2>/dev/null || true; $T -X M4APP 2>/dev/null || true
      $T -F M4VIOLA 2>/dev/null || true; $T -X M4VIOLA 2>/dev/null || true
    done
    ipset destroy m4relay 2>/dev/null || true
    for sock in /run/systemd/resolve/io.systemd.Resolve /run/systemd/resolve/io.systemd.Resolve.Monitor \
                /run/dbus/system_bus_socket /run/avahi-daemon/socket /run/nscd/socket; do
      [ -S "$sock" ] && setfacl -x "u:$UTENTE" "$sock" 2>/dev/null || true
    done
    ;;

  *) echo "uso: rete.sh prepara <file ipset> | controllo | conta | pulisci" >&2; exit 2 ;;
esac
