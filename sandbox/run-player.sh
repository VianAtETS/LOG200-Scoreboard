#!/usr/bin/env bash
# Lance un joueur dans son bac a sable : sans reseau, limites de ressources, systeme de fichiers en
# lecture seule, et seulement la socket de sa couleur. Les variables SANDBOX_* viennent de la section
# [sandbox] de ladder.toml.
# Usage : run-player.sh GAME SIDE PORT FICHIER.jar
#         run-player.sh GAME SIDE PORT --bot random|greedy|aleatoire|niveau-1|niveau-2|niveau-3
set -euo pipefail
game=$1 side=$2 port=$3
shift 3
case $side in
    red) cpuset=$SANDBOX_CPUSET_RED ;;
    black) cpuset=$SANDBOX_CPUSET_BLACK ;;
    *) echo "couleur inconnue : $side" >&2; exit 2 ;;
esac

args=(--rm --name "$game-$side" --label isolation-ladder
    --runtime "$SANDBOX_RUNTIME" --network none --read-only --tmpfs /tmp:size=64m
    --memory "$SANDBOX_MEMORY" --memory-swap "$SANDBOX_MEMORY" --cpus "$SANDBOX_CPUS"
    --cpuset-cpus "$cpuset" --pids-limit "$SANDBOX_PIDS"
    --cap-drop ALL --cap-add SETUID --cap-add SETGID --security-opt no-new-privileges
    -e HOME=/tmp -v "$SANDBOX_RUNDIR/$game/$side:/chan")
# Si le template du lab vise un nom d'hote plutot que localhost.
if [ -n "${SANDBOX_HOST_ALIAS:-}" ]; then
    args+=(--add-host "$SANDBOX_HOST_ALIAS:127.0.0.1")
fi

if [ "$1" = --bot ]; then
    cmd=(isolation-bot --strategy "$2" --port "$port")
else
    args+=(-v "$(realpath "$1"):/bot.jar:ro")
    cmd=(java -XX:-UsePerfData -Djava.net.preferIPv4Stack=true "-Xmx$SANDBOX_JAVA_HEAP" -jar /bot.jar)
fi
exec docker run "${args[@]}" "$SANDBOX_IMAGE" player-entry.sh "$port" "${cmd[@]}"
