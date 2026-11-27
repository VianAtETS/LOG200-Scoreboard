#!/usr/bin/env bash
# Supprime les conteneurs et les sockets d'une partie : tuer un client `docker run` ne suffit pas.
# Sans argument (ou vide), supprime tout ce que le ladder a laisse, par exemple apres un Ctrl+C.
# Usage : cleanup.sh [GAME]
set -uo pipefail
if [ -z "${1:-}" ]; then
    ids=$(docker ps -aq --filter label=isolation-ladder)
    # Une id par mot : la separation est voulue
    # shellcheck disable=SC2086
    [ -n "$ids" ] && docker rm -f $ids >/dev/null
    rm -rf "${SANDBOX_RUNDIR:?}"/*
else
    docker rm -f "$1-referee" "$1-red" "$1-black" >/dev/null 2>&1
    rm -rf "${SANDBOX_RUNDIR:?}/$1"
fi
exit 0
