#!/usr/bin/env bash
# Construit les images des bacs a sable a partir des binaires de ce depot : l'image complete (arbitre
# et bots) et celle des jars, suffixee -jar, sans nos binaires.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --bin isolation-referee
cargo build --profile bot --bin isolation-bot
ctx=$(mktemp -d)
trap 'rm -rf "$ctx"' EXIT
cp target/release/isolation-referee target/bot/isolation-bot sandbox/referee-entry.sh sandbox/player-entry.sh "$ctx/"
image=${SANDBOX_IMAGE:-isolation-sandbox}
docker build -t "$image" -f sandbox/Dockerfile "$ctx"
docker build -t "$image-jar" --target jar -f sandbox/Dockerfile "$ctx"
