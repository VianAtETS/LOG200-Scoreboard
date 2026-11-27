// Formats publiés par le ladder : docs/formats.md.

import type { TChange } from './game.types';

export type TColor = 'red' | 'black';

export type TReason = 'no_moves' | 'timeout' | 'disconnect' | 'protocol' | 'no_show';

// [rating après la partie, variation].
export type TRatingChange = [number, number];

export type TSide = {
  team: string;
  // Absent : bot de référence.
  build?: string;
  remote?: boolean;
  all: TRatingChange;
  session: TRatingChange;
  // Sans les parties distantes, écrit seulement s'il diffère.
  verified?: Partial<Record<TRatingKey, TRatingChange>>;
};

export type TRatingKey = 'all' | 'session';

export type TTournamentRef = {
  id: string;
  match: string;
  game: number;
};

export type TRatingRow = {
  v: number;
  id: string;
  ts: string;
  session: string;
  red: TSide;
  black: TSide;
};

// The compact timeline publishes ratings, not the winner or replay details.
export type TTimelineRow = TRatingRow & { tournament?: true };

// Ligne de summary.json (recent) et de teams/<équipe>.jsonl.
export type TGameRow = TRatingRow & {
  // `null` : nulle (jamais en Isolation).
  winner: TColor | null;
  reason: TReason | string;
  moves: number;
  duration_ms: number;
  time_ms?: number;
  tournament?: TTournamentRef;
  // Jouée sur un nœud de calcul : temps par coup plus longs, hors des statistiques de temps.
  node?: true;
  // Ladder d'avant la base : [début, longueur] de la ligne dans games.jsonl.
  at?: [number, number];
};

// Partie complète de /api/games/<id>.
export type TGame = Omit<TGameRow, 'moves' | 'node'> & {
  // Le jeu et la version de ses règles (résultat v2).
  game?: string;
  game_version?: number;
  // Effets de chaque demi-coup, ouverture comprise : /api/games/<id> les ajoute.
  changes?: TChange[][];
  // Nom du nœud, dans /api/games/<id>.
  node?: string;
  opening?: string[];
  moves: string[];
  move_ms?: number[];
  // Mesures publiques des bots du ladder, une par coup de leur couleur.
  measures?: Partial<Record<TColor, TMeasure[]>>;
};

// Ligne ARENA d'un coup : profondeur terminée (absente sans recherche) et positions visitées.
export type TMeasure = { depth?: number; nodes: number };

// Partie en cours dans live.json.
export type TRunningGame = {
  id?: string;
  red: string;
  black: string;
  tournament?: TTournamentRef;
};

// Un côté d'un duel : l'équipe et son build, absent pour toutes ses versions.
export type TDuelSide = { team: string; build?: string };

// Une paire complète du ladder (GET /api/stats/pairs) : deux parties, même ouverture, couleurs inversées.
export type TDuelPair = {
  games: [string, string];
  // `null` pour une nulle.
  winners: [string | null, string | null];
  opening: string[];
};

// Cartes de chaleur d'un build (GET /api/stats/heatmap) : par case (`F4`), arrivées du pion et cases
// détruites. Une case jamais touchée n'y figure pas ; `build` vaut null pour les bots du ladder.
export type THeatmap = {
  build: string | null;
  games: number;
  moves: Record<string, number>;
  blocks: Record<string, number>;
};
