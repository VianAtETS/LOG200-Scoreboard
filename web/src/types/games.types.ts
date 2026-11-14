// Formats publiés par le ladder : docs/formats.md.

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
  winner: TColor;
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
  // Nom du nœud, dans /api/games/<id>.
  node?: string;
  opening?: string[];
  moves: string[];
  move_ms?: number[];
};

// Partie en cours dans live.json.
export type TRunningGame = {
  id?: string;
  red: string;
  black: string;
  tournament?: TTournamentRef;
};
