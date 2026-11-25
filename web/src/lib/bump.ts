// Rangs de la page graphe, rejoués partie par partie depuis timeline.jsonl (format dans docs/formats.md).
// Un côté sans build est un bot de référence : il ne compte dans le classement que s'il est choisi.

import { isRemote, ratingOf } from '@/lib/elo';
import type { TRatingKey, TRatingRow } from '@/types/games.types';

export const TOP = 10;

export type TRankPoint = {
  x: number;
  rank: number;
  rating: number;
  build?: string | undefined;
  version: boolean;
  bot: boolean;
};

export type TSmoothPoint = { x: number; rank: number };

export type TRankSeriesResult = { columns: TRatingRow[]; series: Map<string, TRankPoint[]> };

export function topTeams(
  ratings: { team: string; build?: string | undefined }[],
  limit = TOP,
): string[] {
  return ratings
    .filter((rating) => rating.build !== undefined && rating.build !== '')
    .slice(0, limit)
    .map((rating) => rating.team);
}

function rankOf(
  team: string,
  rating: number,
  ratings: Map<string, number>,
  counted: (team: string) => boolean,
): number {
  let ahead = 0;
  for (const [other, otherRating] of ratings) {
    if (other === team || !counted(other)) {
      continue;
    }
    if (otherRating > rating || (otherRating === rating && other < team)) {
      ahead += 1;
    }
  }
  return ahead + 1;
}

function updateRatings(
  game: TRatingRow,
  key: TRatingKey,
  verified: boolean,
  {
    ratings,
    bots,
    builds,
  }: { ratings: Map<string, number>; bots: Set<string>; builds: Map<string, string> },
): Map<string, string> {
  const changed = new Map<string, string>();
  for (const side of [game.red, game.black]) {
    ratings.set(side.team, ratingOf(side, key, verified)[0]);
    if (side.build === undefined || side.build === '') {
      bots.add(side.team);
    }
    const before = builds.get(side.team);
    if (side.build !== undefined && side.build !== '' && side.build !== before) {
      builds.set(side.team, side.build);
      if (before !== undefined) {
        changed.set(side.team, side.build);
      }
    }
  }
  return changed;
}

// Une colonne par partie jouée à `cutoff` (ms) ou après, et pour chaque équipe de `selected` un point à
// chaque changement de rang, de rating ou de build. Rang : 1 + équipes et bots choisis devant (rating
// plus haut, puis nom). `verified` : sans les parties distantes.
export function rankSeries(
  rows: TRatingRow[],
  {
    key = 'all',
    session = '',
    selected = [],
    cutoff = -Infinity,
    verified = false,
  }: {
    key?: TRatingKey | undefined;
    session?: string | undefined;
    selected?: string[] | undefined;
    cutoff?: number | undefined;
    verified?: boolean | undefined;
  } = {},
): TRankSeriesResult {
  const ratings = new Map<string, number>();
  const bots = new Set<string>();
  const builds = new Map<string, string>();
  const series = new Map<string, TRankPoint[]>(selected.map((team) => [team, []]));
  const counted = (team: string): boolean => !bots.has(team) || series.has(team);
  const columns: TRatingRow[] = [];
  for (const game of rows) {
    if ((session !== '' && game.session !== session) || (verified && isRemote(game))) {
      continue;
    }
    const changed = updateRatings(game, key, verified, { ratings, bots, builds });
    if (Date.parse(game.ts) < cutoff) {
      continue;
    }
    const x = columns.push(game) - 1;
    for (const [team, points] of series) {
      const rating = ratings.get(team);
      if (rating === undefined) {
        continue;
      }
      const rank = rankOf(team, rating, ratings, counted);
      const last = points.at(-1);
      const version = changed.has(team);
      if (last?.rank === rank && last.rating === rating && !version) {
        continue;
      }
      points.push({ x, rank, rating, build: builds.get(team), version, bot: bots.has(team) });
    }
  }
  return { columns, series };
}

// Dernier point à `x` ou avant, `undefined` avant le départ de la branche.
export function pointAt<T extends { x: number }>(points: readonly T[], x: number): T | undefined {
  let lo = 0;
  let hi = points.length;
  while (lo < hi) {
    const mid = Math.floor((lo + hi) / 2);
    const item = points[mid];
    if (item && item.x <= x) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  return points[lo - 1];
}

// Rang moyen par paquet quand `count` colonnes ne tiennent pas en `buckets`, sinon la série telle quelle.
export function smooth(points: TSmoothPoint[], count: number, buckets: number): TSmoothPoint[] {
  const [first] = points;
  if (count <= buckets || !first) {
    return points;
  }
  const out: TSmoothPoint[] = [];
  const size = count / buckets;
  let i = 0;
  let current = first;
  for (let bucket = 0; bucket < buckets; bucket++) {
    const end = Math.round((bucket + 1) * size);
    const start = Math.max(Math.round(bucket * size), first.x);
    if (start >= end) {
      continue;
    }
    let sum = 0;
    for (let col = start; col < end;) {
      for (let ahead = points[i + 1]; ahead && ahead.x <= col; ahead = points[i + 1]) {
        current = ahead;
        i += 1;
      }
      const ahead = points[i + 1];
      const next = Math.min(ahead?.x ?? end, end);
      sum += current.rank * (next - col);
      col = next;
    }
    out.push({ x: (start + end - 1) / 2, rank: sum / (end - start) });
  }
  return out;
}
