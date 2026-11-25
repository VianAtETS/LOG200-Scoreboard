// Le classement Elo est calcule par le ladder (crates/isolation-ladder/src/standings/).

import type { TGameRow, TRatingChange, TRatingKey, TSide } from '../types/games.types';

// Lignes illisibles (dont une derniere ligne en cours d'ecriture) et versions inconnues ignorees.
export function parseJsonl<T extends { v: number } = TGameRow>(text: string) {
  const games: T[] = [];
  let skipped = 0;
  for (const line of text.split('\n')) {
    if (!line.trim()) {
      continue;
    }
    try {
      const g = JSON.parse(line);
      if (g.v === 1) {
        games.push(g);
      } else {
        skipped++;
      }
    } catch {
      skipped++;
    }
  }
  return { games, skipped };
}

export const BETWEEN = 'entre';

// [rating après la partie, variation] d'un côté, sous `key` (all ou session). `verified` : sans les
// parties distantes, que le ladder n'écrit que s'ils diffèrent.
export function ratingOf(side: TSide, key: TRatingKey, verified = false): TRatingChange {
  return (verified && side.verified?.[key]) || side[key];
}

export const isRemote = (g: Pick<TGameRow, 'red' | 'black'>) =>
  Boolean(g.red.remote || g.black.remote);

// Parties des équipes `selected` dans l'ordre, une seule fois chacune : une partie entre deux équipes
// choisies est dans leurs deux fichiers. BETWEEN : seulement les parties entre équipes choisies.
export function gamesOf(
  rowsByTeam: Map<string, TGameRow[]>,
  selected: string[],
  { mode = '', session = '' }: { mode?: string; session?: string } = {},
) {
  const chosen = new Set(selected);
  const between = mode === BETWEEN && chosen.size > 1;
  const games = new Map<string, TGameRow>();
  for (const team of chosen) {
    for (const g of rowsByTeam.get(team) ?? []) {
      if (session && g.session !== session) {
        continue;
      }
      if (between && !(chosen.has(g.red.team) && chosen.has(g.black.team))) {
        continue;
      }
      games.set(g.id, g);
    }
  }
  return [...games.values()].sort((a, b) => (a.ts < b.ts ? -1 : a.ts > b.ts ? 1 : 0));
}

// Bilan de `team` contre `opponent`, limité à son build `build` (toutes ses parties sans build : bots).
// L'adversaire compte avec toutes ses versions.
export function record(games: TGameRow[], team: string, opponent: string, build?: string) {
  let wins = 0;
  let played = 0;
  for (const g of games) {
    const color = g.red.team === team ? 'red' : g.black.team === team ? 'black' : null;
    if (!color || g[color === 'red' ? 'black' : 'red'].team !== opponent) {
      continue;
    }
    if (build && g[color].build !== build) {
      continue;
    }
    played++;
    if (g.winner === color) {
      wins++;
    }
  }
  return { wins, games: played };
}
