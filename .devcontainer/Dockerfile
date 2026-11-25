FROM oven/bun:1.4.5-slim AS bun

# JDK 25 : compile aussi des jars de test pour Java 24 (javac --release 24)
FROM mcr.microsoft.com/devcontainers/java:25-bookworm

# procps : pkill, utilise par le ladder pour arreter les processus lances par un joueur
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get install -y --no-install-recommends ca-certificates curl procps \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

# gVisor pour isoler les jars des equipes. Le dockerd lance par la feature docker-in-docker lit
# /etc/docker/daemon.json ; --host-uds laisse les bacs a sable partager les sockets Unix montees.
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get install -y --no-install-recommends gnupg \
    && curl -fsSL https://gvisor.dev/archive.key | gpg --dearmor -o /usr/share/keyrings/gvisor-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/gvisor-archive-keyring.gpg] https://storage.googleapis.com/gvisor/releases release main" \
        > /etc/apt/sources.list.d/gvisor.list \
    && apt-get update && apt-get install -y --no-install-recommends runsc socat \
    && apt-get clean && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /etc/docker \
    && printf '{"runtimes":{"runsc-host-uds":{"path":"/usr/bin/runsc","runtimeArgs":["--host-uds=all"]}}}\n' \
        > /etc/docker/daemon.json

COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
RUN ln -s /usr/local/bin/bun /usr/local/bin/bunx \
    && test "$(bun --version)" = "1.4.5"
