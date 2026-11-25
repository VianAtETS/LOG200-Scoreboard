#!/bin/sh
# Sert la page sur /web/. SCOREBOARD_SERVER (ex. https://ladder.example.com, sans / final) la fait
# lire un autre serveur ; sans lui, elle lit le site qui la sert.
set -eu

if [ -n "${SCOREBOARD_SERVER:-}" ]; then
    if ! echo "$SCOREBOARD_SERVER" | grep -Eq '^https?://[^/"]+$'; then
        echo "SCOREBOARD_SERVER invalide : $SCOREBOARD_SERVER (attendu : https://domaine, sans / final)" >&2
        exit 1
    fi
    sed -i "s|<meta content=\"[^\"]*\" name=\"scoreboard-server\"|<meta content=\"$SCOREBOARD_SERVER\" name=\"scoreboard-server\"|" /srv/web/*.html
fi

exec caddy file-server --root /srv --listen :80
