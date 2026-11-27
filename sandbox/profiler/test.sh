#!/usr/bin/env bash
# Verifie le profileur contre un faux serveur, dans un JDK : un client qui lit la socket dans son fil
# principal donne une ligne `prof` par coup, avec sa recursion ; un client a fil lecteur dedie n'en
# donne aucune (le ladder marque son profil douteux).
# Usage : docker run --rm -v "$PWD:/repo:ro" -w /repo eclipse-temurin:24-jdk bash sandbox/profiler/test.sh
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
bash "$here/build.sh" "$work"
moves=5

# Rend la sortie d'erreur du client profile, joue contre le faux serveur sur PORT.
play() {
    local mode=$1 port=$2
    java "$here/test/FakeServer.java" "$port" "$moves" "$work/$mode.ready" &
    local server=$!
    # Le serveur compile sa source avant d'ecouter.
    for _ in $(seq 200); do
        [ -e "$work/$mode.ready" ] && break
        sleep 0.1
    done
    java -javaagent:"$work/profiler.jar" "$here/test/FakeClient.java" "$port" "$mode" 2> "$work/$mode.err"
    wait "$server"
    cat "$work/$mode.err"
}

fail() { echo "ECHEC : $*" >&2; exit 1; }

main=$(play main 7101)
turns=$(grep -c '^prof ' <<< "$main" || true)
[ "$turns" = "$moves" ] || fail "fil principal : $turns lignes prof pour $moves coups"$'\n'"$main"
grep '^prof ' <<< "$main" | grep -q 'FakeClient.search=' || fail "fil principal : FakeClient.search absent"
grep '^prof ' <<< "$main" | grep -qE ' rec=[1-9]' || fail "fil principal : aucune recursion"
grep '^prof ' <<< "$main" | grep -qE ' cost=[0-9]+ end=1 ' || fail "fil principal : cout absent"

reader=$(play reader 7102)
turns=$(grep -c '^prof ' <<< "$reader" || true)
[ "$turns" = 0 ] || fail "fil lecteur dedie : $turns lignes prof au lieu de 0"$'\n'"$reader"

echo "profileur : $moves tours sur $moves coups, aucun avec un fil lecteur dedie"
