import type { TGameRow } from './games.types';

export type TPace = {
  mean?: number;
  p50: number;
  p95: number;
  p99: number;
  max: number;
  count: number;
  limit: number;
};

export type TPaceStat = 'mean' | 'p50' | 'p95' | 'p99' | 'max';

export type TTeamRating = {
  team: string;
  rating: number;
  peak?: number;
  build?: string;
  remote?: boolean;
  anchor?: boolean;
  provisional?: boolean;
  games: number;
  wins: number;
  losses: number;
  build_wins: number;
  build_games: number;
  pace?: TPace;
  last?: string;
};

export type TScope = {
  ratings: TTeamRating[];
  recent: TGameRow[];
  games: number;
};

export type TSummary = {
  sessions: string[];
  // Équipe vers ses builds dans l'ordre, toutes sessions.
  builds: Record<string, string[]>;
  scopes: Record<string, TScope>;
  // Présent une fois une partie distante jouée.
  verified?: Record<string, TScope>;
  // Filtres de versions archivées (ladder lancé avec --archives).
  archives?: boolean;
};

// Modèle de probabilité de model.json.
export type TModel = {
  teams: Record<string, { s: number; n: number }>;
  min_games: number;
  red_advantage: number;
};

export type TQueueTeam = {
  id: string;
  ahead: number;
  bot: boolean;
};

export type TAnnounced = {
  teams: [string, string];
  at: string;
};

export type TQueueFile = {
  teams: TQueueTeam[];
  next?: TAnnounced[];
};
