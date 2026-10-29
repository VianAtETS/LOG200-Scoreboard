#!/usr/bin/env bash
# Demarre le Docker interne, (re)construit l'image des bacs a sable, publie la page pour le service
# web, puis lance le ladder. Les arguments sont passes au ladder (ex. --rounds 10).
set -euo pipefail

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

mkdir -p /srv/site
cp -r /opt/web/. /srv/site/

exec isolation-ladder /srv/ladder.toml "$@"
