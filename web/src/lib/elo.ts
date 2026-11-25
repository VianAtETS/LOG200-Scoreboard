// Le classement Elo est calcule par le ladder (crates/isolation-ladder/src/standings/).

import type { TGameRow, TRatingChange, TRatingKey, TSide } from '@/types/games.types';

// Lignes illisibles (dont une derniere ligne en cours d'ecriture) et versions inconnues ignorees.
// Callers select the published JSONL row format; this parser only checks its version.
// oxlint-disable-next-line typescript/no-unnecessary-type-parameters
export function parseJsonl<T extends { v: number } = TGameRow>(
  text: string,
): { games: T[]; skipped: number } {
  const games: T[] = [];
  let skipped = 0;
  for (const line of text.split('\n')) {
    if (!line.trim()) {
      continue;
    }
    try {
      // The caller supplies the row type for this published JSONL file.
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion
      const game = JSON.parse(line) as T;
      if (game.v === 1) {
        games.push(game);
      } else {
        skipped += 1;
      }
    } catch {
      skipped += 1;
    }
  }
  return { games, skipped };
}

export const BETWEEN = 'entre';

// [rating après la partie, variation] d'un côté, sous `key` (all ou session). `verified` : sans les
// parties distantes, que le ladder n'écrit que s'ils diffèrent.
export function ratingOf(side: TSide, key: TRatingKey, verified = false): TRatingChange {
  return (verified ? side.verified?.[key] : undefined) ?? side[key];
}

export const isRemote = (game: Pick<TGameRow, 'red' | 'black'>): boolean =>
  Boolean(game.red.remote) || Boolean(game.black.remote);

// Parties des équipes `selected` dans l'ordre, une seule fois chacune : une partie entre deux équipes
// choisies est dans leurs deux fichiers. BETWEEN : seulement les parties entre équipes choisies.
export function gamesOf(
  rowsByTeam: Map<string, TGameRow[]>,
  selected: string[],
  { mode = '', session = '' }: { mode?: string; session?: string } = {},
): TGameRow[] {
  const chosen = new Set(selected);
  const between = mode === BETWEEN && chosen.size > 1;
  const games = new Map<string, TGameRow>();
  for (const team of chosen) {
    for (const game of rowsByTeam.get(team) ?? []) {
      if (session && game.session !== session) {
        continue;
      }
      if (between && !(chosen.has(game.red.team) && chosen.has(game.black.team))) {
        continue;
      }
      games.set(game.id, game);
    }
  }
  return [...games.values()].toSorted((left, right) => {
    if (left.ts < right.ts) {
      return -1;
    }
    return left.ts > right.ts ? 1 : 0;
  });
}

// Bilan de `team` contre `opponent`, limité à son build `build` (toutes ses parties sans build : bots).
// L'adversaire compte avec toutes ses versions.
export function record(
  games: TGameRow[],
  team: string,
  opponent: string,
  build?: string,
): { wins: number; games: number } {
  let wins = 0;
  let played = 0;
  for (const game of games) {
    let color: 'red' | 'black' | null = null;
    if (game.red.team === team) {
      color = 'red';
    } else if (game.black.team === team) {
      color = 'black';
    }
    if (!color || game[color === 'red' ? 'black' : 'red'].team !== opponent) {
      continue;
    }
    if (build !== undefined && build !== '' && game[color].build !== build) {
      continue;
    }
    played += 1;
    if (game.winner === color) {
      wins += 1;
    }
  }
  return { wins, games: played };
}
