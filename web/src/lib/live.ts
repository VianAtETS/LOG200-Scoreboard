// Direct des parties en cours : WebSocket /api/live du ladder (protocole dans docs/formats.md).
// Le serveur envoie l'état à la connexion, puis chaque changement ; la page ne lui écrit jamais.

import type { TColor } from '@/types/games.types';
import type { TLiveGame, TLiveGames, TLiveMessage, TLiveState } from '@/types/live.types';

export const LIVE_VERSION = 1;
const RETRY_JITTER = 0.5;
const RETRY_MIN_MS = 1000;
const RETRY_MAX_MS = 60_000;
export const ENDED_KEEP_MS = 30_000;

export function liveUrl(server: string, base: string): string {
  const url = new URL(`${server}/api/live`, base);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  return url.href;
}

export function forget(games: TLiveGames, now: number): TLiveGames {
  const kept = [...games].filter(
    ([, game]) => !game.ended || now - (game.endedAt ?? Number.NaN) < ENDED_KEEP_MS,
  );
  return kept.length === games.size ? games : new Map(kept);
}

function snapshot(games: TLiveGames, incoming: TLiveGame[], now: number): TLiveGames {
  const fresh = new Map(
    incoming.map((game) => [game.id, { ...game, turnAt: now - (game.turn_ms ?? 0) }]),
  );
  for (const [id, game] of games) {
    if (!fresh.has(id)) {
      fresh.set(id, game.ended ? game : { ...game, ended: { interrupted: true }, endedAt: now });
    }
  }
  return fresh;
}

export function apply(
  games: TLiveGames,
  msg: TLiveMessage,
  now: number,
): TLiveGames | 'outdated' | 'resync' {
  if (msg.v !== LIVE_VERSION) {
    return 'outdated';
  }
  const current = forget(games, now);
  const next = new Map(current);
  const live = (game: TLiveGame): TLiveGame => ({ ...game, turnAt: now - (game.turn_ms ?? 0) });
  switch (msg.type) {
    case 'snapshot': {
      return snapshot(current, msg.games, now);
    }
    case 'started': {
      next.set(msg.game.id, live(msg.game));
      return next;
    }
    case 'move': {
      const game = current.get(msg.id);
      if (!game || game.ended) {
        return current;
      }
      if (msg.n !== game.moves.length) {
        return 'resync';
      }
      next.set(msg.id, {
        ...game,
        moves: [...game.moves, msg.move],
        move_ms: [...game.move_ms, msg.ms],
        turnAt: now,
      });
      return next;
    }
    case 'ended': {
      const game = current.get(msg.id);
      if (!game) {
        return current;
      }
      const { winner, reason, cancelled } = msg;
      next.set(msg.id, {
        ...game,
        ended: { winner, reason, cancelled: Boolean(cancelled) },
        endedAt: now,
      });
      return next;
    }
    default: {
      return current;
    }
  }
}

export function pickLive(
  games: TLiveGames,
  teams: string[],
  between: boolean,
  shownId?: string,
): TLiveGame | undefined {
  const both = between && teams.length > 1;
  const plays = (team: string): boolean => teams.includes(team);
  const fits = (game: TLiveGame): boolean =>
    both ? plays(game.red) && plays(game.black) : plays(game.red) || plays(game.black);
  const playing = [...games.values()]
    .filter((game) => !game.ended && fits(game))
    .toSorted((left, right) => (left.since < right.since ? 1 : -1))
    .at(0);
  const shown = shownId !== undefined && shownId !== '' ? games.get(shownId) : undefined;
  return playing ?? (shown && fits(shown) ? shown : undefined);
}

export const mover = (game: TLiveGame): TColor =>
  (game.opening.length + game.moves.length) % 2 === 0 ? 'red' : 'black';

// Rend une fonction qui coupe la connexion et arrête les reconnexions.
export function connectLive(
  url: string,
  onChange: (games: TLiveGames, state: TLiveState) => void,
): () => void {
  let games: TLiveGames = new Map();
  let retry = RETRY_MIN_MS;
  let outdated = false;
  let closed = false;
  let socket: WebSocket;
  let timer: ReturnType<typeof setTimeout>;
  const open = (): void => {
    let joined = false;
    socket = new WebSocket(url);
    socket.addEventListener('message', (event) => {
      let msg: TLiveMessage;
      if (typeof event.data !== 'string') {
        return;
      }
      try {
        // The ladder WebSocket protocol in docs/formats.md is not validated at runtime.
        // oxlint-disable-next-line typescript/no-unsafe-assignment
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
    });
    socket.addEventListener('close', () => {
      if (outdated || closed) {
        return;
      }
      onChange(games, joined ? 'reconnecting' : 'unavailable');
      timer = setTimeout(open, retry * (RETRY_JITTER + Math.random()));
      retry = Math.min(retry * 2, RETRY_MAX_MS);
    });
  };
  open();
  return () => {
    closed = true;
    clearTimeout(timer);
    socket.close();
  };
}
