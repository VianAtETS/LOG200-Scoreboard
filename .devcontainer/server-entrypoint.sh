#!/usr/bin/env bash
# Démarre l'affichage virtuel, le serveur VNC/noVNC puis le serveur de jeu sous Wine.
# Le serveur de jeu est relancé s'il est fermé (bouton "Quitter").
set -euo pipefail

SCREEN="${SCREEN:-1024x864x24}"
EXE="${BOARDGAME_EXE:-/opt/boardgame/BoardGame.exe}"

# Verrou laissé par une exécution précédente du container (redémarrage)
rm -f "/tmp/.X${DISPLAY#:}-lock" "/tmp/.X11-unix/X${DISPLAY#:}"

Xvfb "$DISPLAY" -screen 0 "$SCREEN" -nolisten tcp &
for _ in $(seq 50); do
    [[ -e "/tmp/.X11-unix/X${DISPLAY#:}" ]] && break
    sleep 0.1
done

fluxbox >/dev/null 2>&1 &
x11vnc -display "$DISPLAY" -localhost -forever -shared -nopw -quiet -rfbport 5900 >/dev/null 2>&1 &
websockify --web /usr/share/novnc 6080 localhost:5900 >/dev/null 2>&1 &

echo "Interface du serveur : http://localhost:6080"

while true; do
    if [[ -f "$EXE" ]]; then
        (cd "$(dirname "$EXE")" && wine "$EXE") || true
        echo "Serveur de jeu fermé, redémarrage..."
        wineserver --wait || true
    else
        echo "Introuvable : $EXE (copier le serveur fourni dans le cours dans official-server/)" >&2
    fi
    sleep 2
done
