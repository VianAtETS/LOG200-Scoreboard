import type { TColor, TTournamentRef } from './games.types';

export type TLiveEnded = {
  winner?: TColor | undefined;
  reason?: string | undefined;
  cancelled?: boolean;
  interrupted?: boolean;
};

// Partie suivie par /api/live ; `turnAt` et `endedAt` sont en temps de performance.now().
export type TLiveGame = {
  id: string;
  red: string;
  black: string;
  since: string;
  time_ms: number;
  opening: string[];
  moves: string[];
  move_ms: number[];
  turn_ms?: number;
  tournament?: TTournamentRef;
  turnAt: number;
  ended?: TLiveEnded;
  endedAt?: number;
};

export type TLiveGames = Map<string, TLiveGame>;

export type TLiveState = 'connecting' | 'on' | 'reconnecting' | 'unavailable' | 'outdated';

export type TLiveMessage =
  | { v: number; type: 'snapshot'; games: TLiveGame[] }
  | { v: number; type: 'started'; game: TLiveGame }
  | { v: number; type: 'move'; id: string; n: number; move: string; ms: number }
  | { v: number; type: 'ended'; id: string; winner?: TColor; reason?: string; cancelled?: boolean };
