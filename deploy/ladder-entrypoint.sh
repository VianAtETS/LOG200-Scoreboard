#!/usr/bin/env bash
# Demarre le Docker interne, (re)construit l'image des bacs a sable, puis lance le ladder. Les
# arguments sont passes au ladder (ex. --rounds 10). Avec `node` pour seul argument, lance un noeud
# de calcul (/srv/node.toml) a la place : meme Docker interne, meme image, memes bacs a sable.
set -euo pipefail
trap 'exit 143' TERM

# La base des parties doit survivre au conteneur : sans volume, le ladder importerait data/games.jsonl
# dans une base perdue au prochain `docker compose up -d`. Un noeud n'a pas de base.
if [ "${1:-}" != node ] && ! awk '$5 == "/srv/state" { found = 1 } END { exit !found }' /proc/self/mountinfo; then

    echo "ERREUR : /srv/state n'est pas un volume. Ajouter '- ./state:/srv/state' aux volumes du" \
        "service ladder dans compose.yml (voir deploy/compose.yml), puis relancer." >&2
    exit 1
fi

# Socket local seulement : ni TLS ni port TCP pour le Docker interne.
DOCKER_TLS_CERTDIR='' dockerd-entrypoint.sh dockerd --host=unix:///var/run/docker.sock \
    > /var/log/dockerd.log 2>&1 &
for _ in $(seq 60); do
    docker info > /dev/null 2>&1 && break
    sleep 1
done
if ! docker info > /dev/null 2>&1; then
    cat /var/log/dockerd.log >&2
    exit 1
fi

# Instantane si rien n'a change (cache dans le volume du Docker interne). Les jars tournent dans
# l'image -jar, sans nos binaires.
docker build -q -t isolation-sandbox /opt/sandbox-image > /dev/null
docker build -q -t isolation-sandbox-jar --target jar /opt/sandbox-image > /dev/null

if [ "${1:-}" = node ]; then
    exec isolation-ladder node /srv/node.toml
fi
exec isolation-ladder /srv/ladder.toml "$@"

