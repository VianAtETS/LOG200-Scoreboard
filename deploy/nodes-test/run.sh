#!/usr/bin/env bash
# Banc d'essai des noeuds : inscrit node-a et node-b, puis demarre le ladder et les deux noeuds.
# Ensuite : state/nodes.json, `docker compose logs -f`, http://127.0.0.1:18090/api/games, et
# `docker compose kill node-a` en pleine partie. Arret : docker compose down -v.
set -euo pipefail
cd "$(dirname "$0")"
# Git Bash sous Windows reecrirait /srv en chemin Windows.
export MSYS_NO_PATHCONV=1
rm -rf data state jars node-a.token node-b.token
mkdir -p data state jars
touch node-a.token node-b.token
for node in node-a node-b; do
    docker compose run --rm --no-deps --entrypoint isolation-ladder -w /srv ladder \
        add-node "$node" --nodes jars/nodes.toml | tr -d '\r' > "$node.token"
done
docker compose up -d
