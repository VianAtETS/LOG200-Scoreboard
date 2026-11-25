import { pageUrl } from '@/lib/url';
import { dayFormat, fullFormat, ordinal } from '@/lib/format';
import type { TRankPoint } from '@/lib/bump';
import type { TRatingRow } from '@/types/games.types';
import type { TChartMetric, TGeometry } from './bump.utils';

type TSummary = {
  loading: boolean;
  geom: TGeometry | null;
  selected: string[];
  columns: TRatingRow[];
  metric: TChartMetric;
  series: Map<string, TRankPoint[]>;
};

export function chartSummary({
  loading,
  geom,
  selected,
  columns,
  metric,
  series,
}: TSummary): string {
  if (loading) {
    return 'Chargement du graphe…';
  }
  const [first] = columns;
  const last = columns.at(-1);
  if (!geom || !first || !last) {
    return selected.length === 0
      ? 'Aucune équipe choisie.'
      : 'Aucune partie de ces équipes dans cette fenêtre.';
  }
  const from = dayFormat.format(new Date(first.ts));
  const to = dayFormat.format(new Date(last.ts));
  const measure = metric === 'rank' ? 'Rang' : 'Elo';
  const description = `${measure} de ${geom.drawn.length} équipes sur ${columns.length} parties, du ${from} au ${to}`;
  let leader: { team: string; value: number } | undefined;
  for (const team of geom.drawn) {
    const points = series.get(team);
    const latest = points?.at(-1);
    if (!latest || points?.[0]?.bot === true) {
      continue;
    }
    const value = latest[metric];
    if (!leader || (metric === 'rank' ? value < leader.value : value > leader.value)) {
      leader = { team, value };
    }
  }
  if (!leader) {
    return `${description}.`;
  }
  const value = metric === 'rank' ? ordinal(leader.value) : `${Math.round(leader.value)} Elo`;
  return `${description} ; en tête des équipes suivies : ${leader.team}, ${value}.`;
}

type TOutcome = { winner: string | null; loser: string; matchup: string };

// Session deltas also describe tournament games, whose global deltas are both zero.
export function matchOutcome(game: TRatingRow | null): TOutcome {
  if (!game) {
    return { winner: null, loser: '', matchup: '' };
  }
  const matchup = `${game.red.team} contre ${game.black.team}`;
  const [, red] = game.red.session;
  const [, black] = game.black.session;
  if (red === black) {
    return { winner: null, loser: '', matchup };
  }
  return red > black
    ? { winner: game.red.team, loser: game.black.team, matchup }
    : { winner: game.black.team, loser: game.red.team, matchup };
}

export const gameUrl = (id: string): string => pageUrl('./', { game: id });
export function valueText(
  game: TRatingRow | null | undefined,
  team: string | null,
  rows: { team: string; p: TRankPoint }[],
  result: string,
): string | undefined {
  if (!game) {
    return undefined;
  }
  const focus = rows.find((row) => row.team === team);
  const reading = focus
    ? `${team} ${ordinal(focus.p.rank)}, ${Math.round(focus.p.rating)} Elo, `
    : '';
  return `${reading}${fullFormat.format(new Date(game.ts))}, ${result}`;
}

export function outcomeText(winner: string | null, loser: string, matchup: string): string {
  return winner === null ? matchup : `${winner} bat ${loser}`;
}
