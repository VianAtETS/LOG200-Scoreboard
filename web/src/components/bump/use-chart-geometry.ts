import type { TRankPoint } from '@/lib/bump';
import { useLayoutEffect, useMemo, useState } from 'react';
import type { RefObject } from 'react';
import { chartGeometry } from './bump.utils';
import type { TChartMetric, TGeometry } from './bump.utils';

const MIN_WIDTH = 240;
const MIN_HEIGHT = 200;
const MAX_CACHED_LINES = 10;
type TSize = { width: number; height: number };
type TLine = { team: string; d: string; cached: boolean };

function chartLines(geom: TGeometry | null): TLine[] {
  if (!geom) {
    return [];
  }
  const longestPaths = [...geom.paths]
    .filter(([, path]) => path.length > geom.inner.w)
    .toSorted(([, left], [, right]) => right.length - left.length)
    .slice(0, MAX_CACHED_LINES);
  const cached = new Set(longestPaths.map(([team]) => team));
  return geom.paintOrder.flatMap((team) => {
    const path = geom.paths.get(team);
    if (!path) {
      return [];
    }
    const d = path
      .map(([col, value], index) => `${index === 0 ? 'M' : 'L'}${geom.x(col)},${geom.y(value)}`)
      .join('');
    return [{ team, d, cached: cached.has(team) }];
  });
}

export function useChartGeometry(
  plot: RefObject<HTMLDivElement | null>,
  count: number,
  series: Map<string, TRankPoint[]>,
  selected: string[],
  metric: TChartMetric,
): { geom: TGeometry | null; lines: TLine[]; size: TSize } {
  const [size, setSize] = useState({ width: MIN_WIDTH, height: MIN_HEIGHT });
  useLayoutEffect(() => {
    const node = plot.current;
    const observer = new ResizeObserver(() => {
      const box = node?.getBoundingClientRect();
      if (!box) {
        return;
      }
      setSize({ width: Math.max(MIN_WIDTH, box.width), height: Math.max(MIN_HEIGHT, box.height) });
    });
    if (node) {
      observer.observe(node);
    }
    return () => observer.disconnect();
  }, [plot]);
  const geom = useMemo(
    () => chartGeometry(count, series, selected, metric, size),
    [count, series, selected, metric, size],
  );
  const lines = useMemo(() => chartLines(geom), [geom]);
  return { geom, lines, size };
}
