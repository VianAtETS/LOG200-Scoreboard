#!/bin/sh
# Dans le conteneur d'un joueur. Le jar se connecte a localhost:PORT comme sur le serveur officiel ; un
# relais lance en root transmet a la socket de sa couleur (une seule connexion), puis le jar tourne en
# nobody, sans capacites, et ne peut ni tuer le relais ni ouvrir la socket directement.
# Usage : player-entry.sh PORT COMMANDE...
set -eu
port=$1
shift
socat "TCP-LISTEN:$port,bind=127.0.0.1" UNIX-CONNECT:/chan/sock &
hex=$(printf '%04X' "$port")
for _ in $(seq 100); do
    grep -q ":$hex 00000000:0000 0A" /proc/net/tcp 2>/dev/null && break
    sleep 0.02
done
# Pas d'exec : 125-127 sont les codes de docker run quand le bac a sable n'a pas pu tourner, et le
# ladder annule alors la partie au lieu de compter un no_show. Un jar qui sortirait avec l'un d'eux
# pour faire annuler sa defaite est ramene a 1 ; en nobody, il ne peut pas tuer ce shell.
status=0
setpriv --reuid=65534 --regid=65534 --clear-groups --inh-caps=-all "$@" || status=$?
case $status in 125|126|127) status=1 ;; esac
exit "$status"
