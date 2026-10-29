# Images des bacs a sable, construites par sandbox/build.sh, qui fournit nos binaires et les scripts
# d'entree. L'etape `jar` (isolation-sandbox-jar) fait tourner les jars des equipes : sans nos binaires,
# un jar ne peut pas lancer isolation-bot --strategy niveau-4 a sa place (/tmp est noexec). L'image
# complete (isolation-sandbox) y ajoute l'arbitre et les bots de reference.
FROM eclipse-temurin:24-jre@sha256:8cb2387a28af84cf0db0948d9c67d4480192f4e567027a3963f145d218e8b4f2 AS jar

RUN apt-get update \
    && apt-get install -y --no-install-recommends socat \
    && rm -rf /var/lib/apt/lists/*

COPY player-entry.sh /usr/local/bin/

FROM jar
COPY isolation-referee isolation-bot referee-entry.sh /usr/local/bin/
