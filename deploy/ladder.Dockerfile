# Image du ladder : Docker interne, gVisor, nos binaires et la page. Chaque partie tourne dans des
# bacs a sable gVisor lances par ce Docker interne, d'ou le conteneur privilegie (voir compose.yml).
# Construite depuis la racine du depot : docker build -f deploy/ladder.Dockerfile .
# La CI passe BINARIES=prebuilt pour reprendre les binaires deja compiles par release.yml.
ARG BINARIES=build

FROM rust:1.99.1-alpine3.24 AS compile
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src src
RUN cargo build --release --locked \
    --bin isolation-ladder --bin isolation-referee --bin isolation-check
RUN cargo build --profile bot --locked --bin isolation-bot

FROM scratch AS build
COPY --from=compile /src/target/release/isolation-ladder \
    /src/target/release/isolation-referee \
    /src/target/release/isolation-check /artifacts/
COPY --from=compile /src/target/bot/isolation-bot /artifacts/

# Binaires musl de la CI, un dossier par architecture ; l'artefact perd le bit executable.
FROM scratch AS prebuilt
ARG TARGETARCH
COPY --chmod=755 prebuilt/${TARGETARCH}/ /artifacts/

FROM ${BINARIES} AS binaries

# runsc et ses binaires annexes sont statiques : on les prend dans le depot apt de gVisor, qui ne
# garde que la derniere version (a mettre a jour ici quand elle change).
FROM debian:bookworm-slim AS gvisor
ARG RUNSC_VERSION=20261019.0
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl gnupg \
    && curl -fsSL https://gvisor.dev/archive.key | gpg --dearmor -o /usr/share/keyrings/gvisor.gpg \
    && echo "deb [signed-by=/usr/share/keyrings/gvisor.gpg] https://storage.googleapis.com/gvisor/releases ${RUNSC_VERSION%.*} main" \
    > /etc/apt/sources.list.d/gvisor.list \
    && apt-get update && apt-get install -y --no-install-recommends "runsc=${RUNSC_VERSION}"

FROM docker:29.9.2-dind-alpine3.24
RUN apk add --no-cache bash procps
# runsc cherche ses binaires annexes dans gvisor-bin/, a cote de lui.
COPY --from=gvisor /usr/bin/runsc /usr/local/bin/runsc
COPY --from=gvisor /usr/bin/gvisor-bin /usr/local/bin/gvisor-bin
RUN mkdir -p /etc/docker \
    && printf '{"runtimes":{"runsc-host-uds":{"path":"/usr/local/bin/runsc","runtimeArgs":["--host-uds=all"]}}}\n' \
    > /etc/docker/daemon.json

COPY --from=binaries /artifacts/isolation-ladder /artifacts/isolation-check /usr/local/bin/
# Contexte de l'image des bacs a sable, construite par le Docker interne au demarrage.
COPY sandbox/Dockerfile sandbox/referee-entry.sh sandbox/player-entry.sh /opt/sandbox-image/
COPY --from=binaries /artifacts/isolation-referee /artifacts/isolation-bot /opt/sandbox-image/
COPY sandbox/run-referee.sh sandbox/run-player.sh sandbox/cleanup.sh /srv/sandbox/
COPY web /opt/web
COPY deploy/ladder-entrypoint.sh /usr/local/bin/

# Les chemins relatifs de ladder.toml (data/, jars/, sandbox/) partent d'ici.
WORKDIR /srv
ENTRYPOINT ["ladder-entrypoint.sh"]
