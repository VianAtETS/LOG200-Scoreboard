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
): T => {
  for (const key in stats) {
    if (key === value && Object.hasOwn(stats, key)) {
      return key;
    }
  }
  return fallback;
};

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

export const recordOf = (
  rating: TTeamRating,
  stat: TRecordStat,
): { wins: number; losses: number; games: number } =>
  stat === 'build'
    ? {
        wins: rating.build_wins,
        losses: rating.build_games - rating.build_wins,
        games: rating.build_games,
      }
    : { wins: rating.wins, losses: rating.losses, games: rating.games };

export const winRate = (rating: TTeamRating): number | undefined =>
  rating.build_games ? rating.build_wins / rating.build_games : undefined;

const teamNames = new Intl.Collator('fr', { numeric: true, sensitivity: 'base' });

export function rankingView(
  ratings: readonly TRankingRow[],
  sort: TRankingSort,
  measures: { elo: TEloStat; record: TRecordStat; pace: TPaceStat },
): { rows: TRankingRow[]; ranks: Map<string, number> } {
  const ranks = new Map<string, number>();
  let rank = 0;
  for (const rating of ratings) {
    if (!rating.archived) {
      rank += 1;
      ranks.set(rating.team, rank);
    }
  }
  const value = (rating: TRankingRow): number | undefined => {
    switch (sort.column) {
      case 'rank': {
        return rating.archived ? undefined : ranks.get(rating.team);
      }
      case 'elo': {
        return rating[measures.elo];
      }
      case 'record': {
        return recordOf(rating, measures.record).wins;
      }
      case 'games': {
        return recordOf(rating, measures.record).games;
      }
      case 'pct': {
        return winRate(rating);
      }
      case 'pace': {
        return rating.pace?.[measures.pace];
      }
      case 'last': {
        return rating.last !== undefined && rating.last !== ''
          ? Date.parse(rating.last)
          : undefined;
      }
      case 'team': {
        return Number.NaN;
      }
      default: {
        throw new Error('Unknown ranking column');
      }
    }
  };
  const direction = sort.direction === 'ascending' ? 1 : -1;
  // Stable sorting starts from the official order, never the previous displayed order.
  const rows = ratings.toSorted((left, right) => {
    if (sort.column === 'team') {
      return direction * teamNames.compare(left.team, right.team);
    }
    const av = value(left);
    const bv = value(right);
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
    if (difference !== 0 || sort.column !== 'record') {
      return direction * difference;
    }
    // With equal wins, fewer losses comes first in descending order.
    return (
      direction * (recordOf(right, measures.record).losses - recordOf(left, measures.record).losses)
    );
  });
  return { rows, ranks };
}
