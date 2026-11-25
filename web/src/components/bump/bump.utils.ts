import { smooth } from '@/lib/bump';
import type { TRankPoint } from '@/lib/bump';
import { dayFormat, timeFormat } from '@/lib/format';
import type { TRatingRow } from '@/types/games.types';

const RATING_MARGIN_LEFT = 52;
const RATING_UNIT = 10;
const TICK_SPACING = 28;
const NO_SPREAD = 0.5;
const DAY_MS = 864e5;
const MAX_DATE_TICKS = 5;
const DATE_TICK_SPACING = 100;
const MARGIN = { top: 18, right: 96, bottom: 28, left: 36 };
// Pixels par partie en dessous desquels le rang est lissé, et largeur d'un paquet lissé.
const EXACT_PX = 4;
const BUCKET_PX = 12;
// Couleurs --color-series-1 à --color-series-10, répétées au-delà.
const COLORS = 10;

type TVertex = [number, number];
type TDateTick = { col: number; text: string; anchor: 'start' | 'middle' | 'end' };
export type TGeometry = {
  x: (col: number) => number;
  y: (value: number) => number;
  inner: { w: number; h: number };
  margin: typeof MARGIN;
  width: number;
  height: number;
  ticks: number[];
  drawn: string[];
  paintOrder: string[];
  paths: Map<string, TVertex[]>;
};
export type TChartMetric = 'rank' | 'rating';

// Équipe vers son emplacement de couleur, pour toute la page : une équipe garde sa couleur.
const colors = new Map<string, number>();

// Première couleur libre ; les équipes retirées libèrent la leur.
export function colorOf(team: string, selected: string[]): string {
  const selectedTeams = new Set(selected);
  for (const existing of colors.keys()) {
    if (!selectedTeams.has(existing)) {
      colors.delete(existing);
    }
  }
  let chosen = colors.get(team);
  if (chosen === undefined) {
    const used = new Set(colors.values());
    let slot = 0;
    while (used.has(slot) && slot < COLORS) {
      slot += 1;
    }
    chosen = slot < COLORS ? slot : colors.size % COLORS;
    colors.set(team, chosen);
  }
  return `var(--color-series-${chosen + 1})`;
}

// Sommets [colonne, rang ou Elo] de la ligne. Valeur exacte : palier, puis diagonale sur une partie.
function vertices(
  points: TRankPoint[],
  count: number,
  buckets: number,
  metric: TChartMetric,
): TVertex[] {
  const [first] = points;
  if (!first) {
    return [];
  }
  const path: TVertex[] = [];
  if (metric === 'rating' || count <= buckets) {
    let previous: TRankPoint | undefined;
    for (const point of points) {
      if (previous) {
        path.push([Math.max(point.x - 1, previous.x), previous[metric]]);
      }
      path.push([point.x, point[metric]]);
      previous = point;
    }
  } else {
    const line = smooth(points, count, buckets);
    path.push(
      [first.x, line[0]?.rank ?? first.rank],
      ...line.map((point): TVertex => [point.x, point.rank]),
    );
  }
  path.push([count - 1, path.at(-1)?.[1] ?? first[metric]]);
  return path;
}

export function chartGeometry(
  count: number,
  series: Map<string, TRankPoint[]>,
  selected: string[],
  metric: TChartMetric,
  { width, height }: { width: number; height: number },
): TGeometry | null {
  const drawn = selected.filter((team) => (series.get(team)?.length ?? 0) > 0);
  if (count === 0 || drawn.length === 0) {
    return null;
  }

  const margin = { ...MARGIN, left: metric === 'rating' ? RATING_MARGIN_LEFT : MARGIN.left };
  let min = Infinity;
  let max = -Infinity;
  for (const team of drawn) {
    for (const point of series.get(team) ?? []) {
      min = Math.min(min, point[metric]);
      max = Math.max(max, point[metric]);
    }
  }
  const inner = { w: width - margin.left - margin.right, h: height - margin.top - margin.bottom };
  const unit = metric === 'rating' ? RATING_UNIT : 1;
  const tickCount = Math.max(1, Math.floor(inner.h / TICK_SPACING));
  const step = Math.max(unit, Math.ceil((max - min) / tickCount / unit) * unit);
  const low = metric === 'rank' ? Math.floor(min) : Math.floor(min / step) * step;
  const high = metric === 'rank' ? Math.ceil(max) : Math.ceil(max / step) * step;
  const ticks = Array.from(
    { length: Math.floor((high - low) / step) + 1 },
    (_, index) => low + index * step,
  );
  const x = (col: number): number =>
    margin.left + (count > 1 ? (col / (count - 1)) * inner.w : inner.w / 2);
  const y = (value: number): number => {
    const fraction = high > low ? (value - low) / (high - low) : NO_SPREAD;
    return margin.top + (metric === 'rank' ? fraction : 1 - fraction) * inner.h;
  };
  const buckets = count * EXACT_PX <= inner.w ? count : Math.floor(inner.w / BUCKET_PX);
  const paths = new Map<string, TVertex[]>();
  for (const team of drawn) {
    paths.set(team, vertices(series.get(team) ?? [], count, buckets, metric));
  }
  return {
    x,
    y,
    inner,
    margin,
    width,
    height,
    ticks,
    drawn,
    paintOrder: drawn.toReversed(),
    paths,
  };
}

// Rang ou Elo interpolé de la ligne à `col` : un nœud se pose sur la ligne, même lissée.
export function valueAt(path: TVertex[], col: number): number {
  let lo = 0;
  let hi = path.length;
  while (lo < hi) {
    const mid = Math.floor((lo + hi) / 2);
    if ((path[mid]?.[0] ?? Infinity) >= col) {
      hi = mid;
    } else {
      lo = mid + 1;
    }
  }
  const previous = path[lo - 1];
  const next = path[lo];
  if (!previous || !next) {
    return path[0]?.[1] ?? Number.NaN;
  }
  const [leftCol, leftValue] = previous;
  const [rightCol, rightValue] = next;
  return rightCol === leftCol
    ? rightValue
    : leftValue + ((rightValue - leftValue) * (col - leftCol)) / (rightCol - leftCol);
}

export function dateTicks(columns: TRatingRow[], width: number): TDateTick[] {
  const [first] = columns;
  const last = columns.at(-1);
  if (!first || !last) {
    return [];
  }
  const span = Date.parse(last.ts) - Date.parse(first.ts);
  const format = span < DAY_MS ? timeFormat : dayFormat;
  const count = Math.min(
    MAX_DATE_TICKS,
    columns.length,
    Math.max(2, Math.floor(width / DATE_TICK_SPACING)),
  );
  const ticks: TDateTick[] = [];
  let previous = '';
  for (let index = 0; index < count; index += 1) {
    const col = count > 1 ? Math.round((index * (columns.length - 1)) / (count - 1)) : 0;
    const column = columns[col];
    if (!column) {
      continue;
    }
    const text = format.format(new Date(column.ts));
    if (text === previous) {
      continue;
    }
    previous = text;
    const anchor = index === count - 1 ? 'end' : 'middle';
    ticks.push({ col, text, anchor: index === 0 ? 'start' : anchor });
  }
  return ticks;
}
