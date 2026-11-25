import { smooth, type TRankPoint } from '../../lib/bump.ts';
import { dayFormat, timeFormat } from '../../lib/format.ts';
import type { TRatingRow } from '@/types/games.types';

const MARGIN = { top: 18, right: 96, bottom: 28, left: 36 };
// Pixels par partie en dessous desquels le rang est lissé, et largeur d'un paquet lissé.
const EXACT_PX = 4;
const BUCKET_PX = 12;
// Couleurs --color-series-1 à --color-series-10, répétées au-delà.
const COLORS = 10;

type TVertex = [number, number];
export type TChartMetric = 'rank' | 'rating';

// Équipe vers son emplacement de couleur, pour toute la page : une équipe garde sa couleur.
const colors = new Map<string, number>();

// Première couleur libre ; les équipes retirées libèrent la leur.
export function colorOf(team: string, selected: string[]) {
  for (const t of colors.keys()) if (!selected.includes(t)) colors.delete(t);
  if (!colors.has(team)) {
    const used = new Set(colors.values());
    let slot = 0;
    while (used.has(slot) && slot < COLORS) slot++;
    colors.set(team, slot < COLORS ? slot : colors.size % COLORS);
  }
  return `var(--color-series-${colors.get(team)! + 1})`;
}

// Sommets [colonne, rang ou Elo] de la ligne. Valeur exacte : palier, puis diagonale sur une partie.
function vertices(points: TRankPoint[], count: number, buckets: number, metric: TChartMetric) {
  const path: TVertex[] = [];
  if (metric === 'rating' || count <= buckets) {
    points.forEach((p, i) => {
      if (i) path.push([Math.max(p.x - 1, points[i - 1].x), points[i - 1][metric]]);
      path.push([p.x, p[metric]]);
    });
  } else {
    const line = smooth(points, count, buckets);
    path.push([points[0].x, line[0].rank], ...line.map((p): TVertex => [p.x, p.rank]));
  }
  path.push([count - 1, path[path.length - 1][1]]);
  return path;
}

export function chartGeometry(
  count: number,
  series: Map<string, TRankPoint[]>,
  selected: string[],
  metric: TChartMetric,
  { width, height }: { width: number; height: number },
) {
  const drawn = selected.filter((t) => series.get(t)?.length);
  if (!count || !drawn.length) return null;

  const margin = { ...MARGIN, left: metric === 'rating' ? 52 : MARGIN.left };
  let min = Infinity;
  let max = -Infinity;
  for (const team of drawn) {
    for (const point of series.get(team)!) {
      min = Math.min(min, point[metric]);
      max = Math.max(max, point[metric]);
    }
  }
  const inner = { w: width - margin.left - margin.right, h: height - margin.top - margin.bottom };
  const unit = metric === 'rating' ? 10 : 1;
  const step = Math.max(
    unit,
    Math.ceil((max - min) / Math.max(1, Math.floor(inner.h / 28)) / unit) * unit,
  );
  const low = metric === 'rank' ? Math.floor(min) : Math.floor(min / step) * step;
  const high = metric === 'rank' ? Math.ceil(max) : Math.ceil(max / step) * step;
  const ticks = Array.from(
    { length: Math.floor((high - low) / step) + 1 },
    (_, i) => low + i * step,
  );
  const x = (col: number) =>
    margin.left + (count > 1 ? (col / (count - 1)) * inner.w : inner.w / 2);
  const y = (value: number) => {
    const fraction = high > low ? (value - low) / (high - low) : 0.5;
    return margin.top + (metric === 'rank' ? fraction : 1 - fraction) * inner.h;
  };
  const buckets = count * EXACT_PX <= inner.w ? count : Math.floor(inner.w / BUCKET_PX);
  const paths = new Map<string, TVertex[]>();
  for (const team of drawn) paths.set(team, vertices(series.get(team)!, count, buckets, metric));
  return {
    x,
    y,
    inner,
    margin,
    width,
    height,
    ticks,
    drawn,
    paintOrder: [...drawn].reverse(),
    paths,
  };
}

// Rang ou Elo interpolé de la ligne à `col` : un nœud se pose sur la ligne, même lissée.
export function valueAt(path: TVertex[], col: number) {
  let lo = 0;
  let hi = path.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (path[mid][0] >= col) hi = mid;
    else lo = mid + 1;
  }
  if (lo === 0 || lo === path.length) return path[0][1];
  const [[c0, r0], [c1, r1]] = [path[lo - 1], path[lo]];
  return c1 === c0 ? r1 : r0 + ((r1 - r0) * (col - c0)) / (c1 - c0);
}

export function dateTicks(columns: TRatingRow[], width: number) {
  const span = Date.parse(columns[columns.length - 1].ts) - Date.parse(columns[0].ts);
  const format = span < 864e5 ? timeFormat : dayFormat;
  const count = Math.min(5, columns.length, Math.max(2, Math.floor(width / 100)));
  const ticks: { col: number; text: string; anchor: 'start' | 'middle' | 'end' }[] = [];
  let previous = '';
  for (let k = 0; k < count; k++) {
    const col = count > 1 ? Math.round((k * (columns.length - 1)) / (count - 1)) : 0;
    const text = format.format(new Date(columns[col].ts));
    if (text === previous) continue;
    previous = text;
    ticks.push({ col, text, anchor: k === 0 ? 'start' : k === count - 1 ? 'end' : 'middle' });
  }
  return ticks;
}
