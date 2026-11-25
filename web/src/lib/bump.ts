// Rangs de la page graphe, rejoués partie par partie depuis timeline.jsonl (format dans docs/formats.md).
// Un côté sans build est un bot de référence : il ne compte dans le classement que s'il est choisi.

import type { TRatingKey, TRatingRow } from '../types/games.types';
import { isRemote, ratingOf } from './elo.ts';

export const TOP = 10;

export type TRankPoint = {
  x: number;
  rank: number;
  rating: number;
  build?: string;
  version: boolean;
  bot: boolean;
};

type TSmoothPoint = { x: number; rank: number };

export function topTeams(ratings: { team: string; build?: string }[], n = TOP) {
  return ratings
    .filter((r) => r.build)
    .slice(0, n)
    .map((r) => r.team);
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
    key?: TRatingKey;
    session?: string;
    selected?: string[];
    cutoff?: number;
    verified?: boolean;
  } = {},
) {
  const ratings = new Map<string, number>();
  const bots = new Set<string>();
  const counted = (team: string) => !bots.has(team) || series.has(team);
  const builds = new Map<string, string>();
  const series = new Map<string, TRankPoint[]>(selected.map((team) => [team, []]));
  const columns: TRatingRow[] = [];
  for (const g of rows) {
    if ((session && g.session !== session) || (verified && isRemote(g))) {
      continue;
    }
    const changed = new Map<string, string>();
    for (const side of [g.red, g.black]) {
      ratings.set(side.team, ratingOf(side, key, verified)[0]);
      if (!side.build) {
        bots.add(side.team);
      }
      const before = builds.get(side.team);
      if (side.build && side.build !== before) {
        builds.set(side.team, side.build);
        if (before) {
          changed.set(side.team, side.build);
        }
      }
    }
    if (Date.parse(g.ts) < cutoff) {
      continue;
    }
    const x = columns.push(g) - 1;
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

function rankOf(
  team: string,
  rating: number,
  ratings: Map<string, number>,
  counted: (team: string) => boolean,
) {
  let ahead = 0;
  for (const [other, r] of ratings) {
    if (other === team || !counted(other)) {
      continue;
    }
    if (r > rating || (r === rating && other < team)) {
      ahead++;
    }
  }
  return ahead + 1;
}

// Dernier point à `x` ou avant, `undefined` avant le départ de la branche.
export function pointAt<T extends { x: number }>(points: T[], x: number): T | undefined {
  let lo = 0;
  let hi = points.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (points[mid].x <= x) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  return points[lo - 1];
}

// Rang moyen par paquet quand `count` colonnes ne tiennent pas en `buckets`, sinon la série telle quelle.
export function smooth(points: TRankPoint[], count: number, buckets: number): TSmoothPoint[] {
  if (count <= buckets || points.length === 0) {
    return points;
  }
  const out: TSmoothPoint[] = [];
  const size = count / buckets;
  let i = 0;
  for (let b = 0; b < buckets; b++) {
    const end = Math.round((b + 1) * size);
    const start = Math.max(Math.round(b * size), points[0].x);
    if (start >= end) {
      continue;
    }
    let sum = 0;
    for (let col = start; col < end;) {
      while (i + 1 < points.length && points[i + 1].x <= col) {
        i++;
      }
      const next = Math.min(points[i + 1]?.x ?? end, end);
      sum += points[i].rank * (next - col);
      col = next;
    }
    out.push({ x: (start + end - 1) / 2, rank: sum / (end - start) });
  }
  return out;
}
