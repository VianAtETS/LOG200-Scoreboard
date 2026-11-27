#!/usr/bin/env bash
# Compile le profileur en OUT/profiler.jar (agent Java, Premain-Class: Profiler). Demande un JDK :
# l'etape `profiler` de deploy/ladder.Dockerfile, ou eclipse-temurin:24-jdk.
# Usage : sandbox/profiler/build.sh OUT
set -euo pipefail
src=$(cd "$(dirname "$0")" && pwd)
out=$1
classes=$(mktemp -d)
trap 'rm -rf "$classes"' EXIT
javac --release 21 -d "$classes" "$src/Profiler.java"
printf 'Premain-Class: Profiler\n' > "$classes/manifest.txt"
mkdir -p "$out"
jar --create --file "$out/profiler.jar" --manifest "$classes/manifest.txt" -C "$classes" Profiler.class
