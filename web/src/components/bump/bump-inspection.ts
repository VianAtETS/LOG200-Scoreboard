import { pointAt } from '@/lib/bump';
import type { TRankPoint } from '@/lib/bump';
import { valueAt } from './bump.utils';
import type { TChartMetric, TGeometry } from './bump.utils';

export type TStandingRow = { team: string; p: TRankPoint; before: TRankPoint | undefined };

export const clampColumn = (col: number, count: number): number =>
  Math.max(0, Math.min(count - 1, col));
// Rang ou Elo exact, pas lissé, meilleure d'abord.
export function standingsAt(
  teams: string[],
  series: Map<string, TRankPoint[]>,
  metric: TChartMetric,
  col: number,
): TStandingRow[] {
  const rows: TStandingRow[] = [];
  for (const team of teams) {
    const points = series.get(team);
    if (!points) {
      continue;
    }
    const point = pointAt(points, col);
    if (point) {
      rows.push({ team, p: point, before: pointAt(points, col - 1) });
    }
  }
  return rows.toSorted((left, right) =>
    metric === 'rank'
      ? left.p.rank - right.p.rank
      : right.p.rating - left.p.rating || left.p.rank - right.p.rank,
  );
}

export function targetAt(
  geom: TGeometry,
  count: number,
  horizontal: number,
  vertical: number,
): { col: number; team: string | null } {
  const col = clampColumn(
    Math.round(((horizontal - geom.margin.left) / geom.inner.w) * (count - 1)),
    count,
  );
  let team: string | null = null;
  let closest = Infinity;
  for (const [candidate, path] of geom.paths) {
    if ((path[0]?.[0] ?? Infinity) > col) {
      continue;
    }
    const distance = Math.abs(geom.y(valueAt(path, col)) - vertical);
    if (distance < closest) {
      team = candidate;
      closest = distance;
    }
  }
  return { col, team };
}
