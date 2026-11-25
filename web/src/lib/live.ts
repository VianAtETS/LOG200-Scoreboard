// Direct des parties en cours : WebSocket /api/live du ladder (protocole dans docs/formats.md).
// Le serveur envoie l'état à la connexion, puis chaque changement ; la page ne lui écrit jamais.

import type { TColor } from '../types/games.types';
import type { TLiveGame, TLiveGames, TLiveMessage, TLiveState } from '../types/live.types';

export const LIVE_VERSION = 1;
const RETRY_MIN_MS = 1000;
const RETRY_MAX_MS = 60000;
export const ENDED_KEEP_MS = 30000;

export function liveUrl(server: string, base: string) {
  const url = new URL(`${server}/api/live`, base);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  return url.href;
}

export function apply(
  games: TLiveGames,
  msg: TLiveMessage,
  now: number,
): TLiveGames | 'outdated' | 'resync' {
  if (msg.v !== LIVE_VERSION) return 'outdated';
  games = forget(games, now);
  const next = new Map(games);
  const live = (g: TLiveGame): TLiveGame => ({ ...g, turnAt: now - (g.turn_ms ?? 0) });
  switch (msg.type) {
    case 'snapshot': {
      const fresh = new Map(msg.games.map((g) => [g.id, live(g)]));
      for (const [id, g] of games) {
        if (fresh.has(id)) continue;
        fresh.set(id, g.ended ? g : { ...g, ended: { interrupted: true }, endedAt: now });
      }
      return fresh;
    }
    case 'started':
      next.set(msg.game.id, live(msg.game));
      return next;
    case 'move': {
      const g = games.get(msg.id);
      if (!g || g.ended) return games;
      if (msg.n !== g.moves.length) return 'resync';
      next.set(msg.id, {
        ...g,
        moves: [...g.moves, msg.move],
        move_ms: [...g.move_ms, msg.ms],
        turnAt: now,
      });
      return next;
    }
    case 'ended': {
      const g = games.get(msg.id);
      if (!g) return games;
      const { winner, reason, cancelled } = msg;
      next.set(msg.id, {
        ...g,
        ended: { winner, reason, cancelled: Boolean(cancelled) },
        endedAt: now,
      });
      return next;
    }
    default:
      return games;
  }
}

export function forget(games: TLiveGames, now: number): TLiveGames {
  const kept = [...games].filter(([, g]) => !g.ended || now - g.endedAt! < ENDED_KEEP_MS);
  return kept.length === games.size ? games : new Map(kept);
}

export function pickLive(games: TLiveGames, teams: string[], between: boolean, shownId?: string) {
  const both = between && teams.length > 1;
  const plays = (team: string) => teams.includes(team);
  const fits = (g: TLiveGame) =>
    both ? plays(g.red) && plays(g.black) : plays(g.red) || plays(g.black);
  const playing = [...games.values()]
    .filter((g) => !g.ended && fits(g))
    .sort((a, b) => (a.since < b.since ? 1 : -1))[0];
  const shown = shownId ? games.get(shownId) : undefined;
  return playing ?? (shown && fits(shown) ? shown : undefined);
}

export const mover = (g: TLiveGame): TColor =>
  (g.opening.length + g.moves.length) % 2 === 0 ? 'red' : 'black';

// Rend une fonction qui coupe la connexion et arrête les reconnexions.
export function connectLive(url: string, onChange: (games: TLiveGames, state: TLiveState) => void) {
  let games: TLiveGames = new Map();
  let retry = RETRY_MIN_MS;
  let outdated = false;
  let closed = false;
  let socket: WebSocket;
  let timer: ReturnType<typeof setTimeout>;
  const open = () => {
    let joined = false;
    socket = new WebSocket(url);
    socket.onmessage = (event) => {
      let msg;
      try {
        msg = JSON.parse(event.data);
      } catch {
        return;
      }
      const next = apply(games, msg, performance.now());
      if (next === 'outdated') {
        outdated = true;
        socket.close();
        onChange(games, 'outdated');
      } else if (next === 'resync') {
        socket.close();
      } else {
        joined = true;
        retry = RETRY_MIN_MS;
        games = next;
        onChange(games, 'on');
      }
    };
    socket.onclose = () => {
      if (outdated || closed) return;
      onChange(games, joined ? 'reconnecting' : 'unavailable');
      timer = setTimeout(open, retry * (0.5 + Math.random()));
      retry = Math.min(retry * 2, RETRY_MAX_MS);
    };
  };
  open();
  return () => {
    closed = true;
    clearTimeout(timer);
    socket.close();
  };
}
