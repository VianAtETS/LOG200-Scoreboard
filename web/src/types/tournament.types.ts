export type TBracketSide = 'winners' | 'losers' | 'final';

export type TMatchStatus = 'pending' | 'running' | 'suspended' | 'done';

export type TMatchSource = {
  seed?: number;
  winner?: string;
  loser?: string;
};

export type TMatch = {
  id: string;
  side: TBracketSide;
  round: number;
  teams: [string | null, string | null];
  status: TMatchStatus;
  winner?: string | null;
  score: [number, number];
  head_start: [number, number];
  best_of: number;
  games: string[];
  from: TMatchSource[];
};

export type TEntrant = {
  team: string;
  build?: string;
  rating: number;
  multithread?: boolean;
};

export type TTournament = {
  id: string;
  ts: string;
  ended?: string;
  winner?: string;
  time_ms: number;
  entrants: TEntrant[];
  places?: number[];
  matches: TMatch[];
  game_ms?: number[];
  // Slots [simples, multithread].
  slots?: [number, number];
};

export type TTournamentIndex = {
  next: string | null;
  tournaments: { id: string; ts: string; winner?: string }[];
};
