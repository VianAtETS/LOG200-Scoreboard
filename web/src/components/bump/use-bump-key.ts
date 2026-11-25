import type { TRankPoint } from '@/lib/bump';
import type { TRatingRow } from '@/types/games.types';
import { useCallback } from 'react';
import type { KeyboardEvent } from 'react';
import { standingsAt } from './bump-inspection';
import type { TChartMetric, TGeometry } from './bump.utils';
import { gameUrl } from './bump-summary';

type TOptions = {
  geom: TGeometry | null;
  col: number | null;
  columns: TRatingRow[];
  show: (col: number, team?: string | null) => void;
  series: Map<string, TRankPoint[]>;
  metric: TChartMetric;
  team: string | null;
  cancel: () => void;
  hide: () => void;
};
const PAGE_DIVISOR = 10;
export function useBumpKey({
  geom,
  col,
  columns,
  show,
  series,
  metric,
  team,
  cancel,
  hide,
}: TOptions): (event: KeyboardEvent<SVGSVGElement>) => void {
  const key = useCallback(
    (event: KeyboardEvent<SVGSVGElement>): void => {
      if (!geom) {
        return;
      }
      const current = col ?? columns.length - 1;
      const page = Math.max(1, Math.round(columns.length / PAGE_DIVISOR));
      const moves: Record<string, number> = {
        ArrowLeft: current - 1,
        ArrowRight: current + 1,
        PageUp: current - page,
        PageDown: current + page,
        Home: 0,
        End: columns.length - 1,
      };
      const destination = moves[event.key];
      if (destination !== undefined) {
        show(destination);
      } else if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
        const rows = standingsAt(geom.drawn, series, metric, current);
        const index =
          rows.findIndex((row) => row.team === team) + (event.key === 'ArrowUp' ? -1 : 1);
        show(current, rows[Math.max(0, Math.min(rows.length - 1, index))]?.team ?? null);
      } else if (event.key === 'Enter' || event.key === ' ') {
        cancel();
        const column = columns[current];
        if (column) {
          location.assign(gameUrl(column.id));
        }
      } else if (event.key === 'Escape') {
        hide();
      } else {
        return;
      }
      event.preventDefault();
    },
    [geom, col, columns, show, series, metric, team, cancel, hide],
  );
  return key;
}
