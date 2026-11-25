import type { TRankingRow } from '@/lib/archives';
import type { TPaceStat, TTeamRating } from '@/types/summary.types';

export const PACE_STATS: Record<TPaceStat, [label: string, title: string]> = {
  mean: ['moy.', 'Temps moyen par coup'],
  p50: ['méd.', 'Temps médian par coup'],
  p95: ['p95', '95e centile du temps par coup'],
  p99: ['p99', '99e centile du temps par coup'],
  max: ['max', 'Temps maximal par coup'],
};

export const RECORD_STATS = {
  all: ['V–D', 'Victoires et défaites, toutes versions'],
  build: ['V–D v.', 'Victoires et défaites, version actuelle'],
} as const;

export const ELO_STATS = {
  rating: ['Elo', 'Elo actuel'],
  peak: ['Peak', 'Elo maximal atteint'],
} as const;

export type TRecordStat = keyof typeof RECORD_STATS;
export type TEloStat = keyof typeof ELO_STATS;

// p99 au-delà de cette part du temps par coup : l'équipe frôle le timeout.
export const PACE_WARN = 0.8;

export const DEFAULT_STATS = { pace: 'p95', record: 'all', elo: 'rating' } as const;

export const statOf = <T extends string>(
  stats: Record<T, unknown>,
  value: string | null,
  fallback: T,
): T => (value !== null && Object.hasOwn(stats, value) ? (value as T) : fallback);

export const SORT_COLUMNS = {
  rank: 'Rang officiel',
  team: 'Équipe',
  elo: 'Elo',
  record: 'Victoires et défaites',
  games: 'Parties',
  pct: '% V',
  pace: 'Temps par coup',
  last: 'Dernière partie',
} as const;

export type TSortColumn = keyof typeof SORT_COLUMNS;
export type TRankingSort = { column: TSortColumn; direction: 'ascending' | 'descending' };

export const sortOf = (column: string | null, order: string | null): TRankingSort => ({
  column: statOf(SORT_COLUMNS, column, 'elo'),
  direction: order === 'asc' ? 'ascending' : 'descending',
});

export const recordOf = (r: TTeamRating, stat: TRecordStat) =>
  stat === 'build'
    ? { wins: r.build_wins, losses: r.build_games - r.build_wins, games: r.build_games }
    : { wins: r.wins, losses: r.losses, games: r.games };

export const winRate = (r: TTeamRating) =>
  r.build_games ? r.build_wins / r.build_games : undefined;

const teamNames = new Intl.Collator('fr', { numeric: true, sensitivity: 'base' });

export function rankingView(
  ratings: readonly TRankingRow[],
  sort: TRankingSort,
  measures: { elo: TEloStat; record: TRecordStat; pace: TPaceStat },
) {
  const ranks = new Map(ratings.filter((r) => !r.archived).map((r, i) => [r.team, i + 1]));
  const value = (r: TRankingRow) => {
    switch (sort.column) {
      case 'rank': {
        return r.archived ? undefined : ranks.get(r.team);
      }
      case 'elo': {
        return r[measures.elo];
      }
      case 'record': {
        return recordOf(r, measures.record).wins;
      }
      case 'games': {
        return recordOf(r, measures.record).games;
      }
      case 'pct': {
        return winRate(r);
      }
      case 'pace': {
        return r.pace?.[measures.pace];
      }
      case 'last': {
        return r.last ? Date.parse(r.last) : undefined;
      }
      case 'team': {
        return undefined;
      }
    }
  };
  const direction = sort.direction === 'ascending' ? 1 : -1;
  // Stable sorting starts from the official order, never the previous displayed order.
  const rows = [...ratings].sort((a, b) => {
    if (sort.column === 'team') {
      return direction * teamNames.compare(a.team, b.team);
    }
    const av = value(a);
    const bv = value(b);
    const missingA = av === undefined || !Number.isFinite(av);
    const missingB = bv === undefined || !Number.isFinite(bv);
    if (missingA && missingB) {
      return 0;
    }
    if (missingA) {
      return 1;
    }
    if (missingB) {
      return -1;
    }
    const difference = av - bv;
    if (difference || sort.column !== 'record') {
      return direction * difference;
    }
    // With equal wins, fewer losses comes first in descending order.
    return direction * (recordOf(b, measures.record).losses - recordOf(a, measures.record).losses);
  });
  return { rows, ranks };
}
