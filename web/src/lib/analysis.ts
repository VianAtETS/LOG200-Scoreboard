// Analyse d'une partie, portée de l'arène isola de Justin Pagès (gitlab.com/justin-pages/isola, arena/web/core.js
// et app.js) : mobilité, territoire et région de chaque couleur après chaque coup, coups critiques et point de bascule.

import type { TColor } from '../types/games.types';
import { BLACK, EMPTY, RED, SIZE, type TStep } from './replay.ts';

export type TSeries = { mob: number[]; terr: number[]; reg: number[] };

export type TAnalysis = {
  red: TSeries;
  black: TSeries;
  // Coup -> 'fine' s'il a gagné du territoire à son auteur, 'bad' s'il lui en a coûté.
  crit: Map<number, 'fine' | 'bad'>;
  // Coup après lequel le perdant n'a plus jamais repris l'avantage en territoire.
  tip: number | null;
};

const DIRS = [[-1, -1], [0, -1], [1, -1], [-1, 0], [1, 0], [-1, 1], [0, 1], [1, 1]];
// Écart de territoire minimal d'un coup critique, et leur nombre maximal.
const SWING = 8;
const CRITICAL = 8;

// Nombre de tours pour atteindre chaque case (-1 : inaccessible), un tour = jusqu'à 3 cases en ligne droite.
function distances(cells: number[], pawn: number) {
  const dist = new Array<number>(SIZE * SIZE).fill(-1);
  const start = cells.indexOf(pawn);
  const queue = [start];
  dist[start] = 0;
  for (let h = 0; h < queue.length; h++) {
    const cur = queue[h];
    for (const [dx, dy] of DIRS) {
      for (let s = 1, x = (cur % SIZE) + dx, y = Math.floor(cur / SIZE) + dy; s <= 3; s++, x += dx, y += dy) {
        if (x < 0 || x >= SIZE || y < 0 || y >= SIZE || cells[y * SIZE + x] !== EMPTY) break;
        const i = y * SIZE + x;
        if (dist[i] < 0) {
          dist[i] = dist[cur] + 1;
          queue.push(i);
        }
      }
    }
  }
  return dist;
}

// Mobilité (cases à un tour), région (cases accessibles) et territoire (cases atteintes avant l'adversaire).
function measure(cells: number[]) {
  const r = distances(cells, RED);
  const b = distances(cells, BLACK);
  const m = { red: { mob: 0, terr: 0, reg: 0 }, black: { mob: 0, terr: 0, reg: 0 } };
  for (let i = 0; i < SIZE * SIZE; i++) {
    if (r[i] === 1) m.red.mob++;
    if (b[i] === 1) m.black.mob++;
    if (r[i] > 0) m.red.reg++;
    if (b[i] > 0) m.black.reg++;
    // Une case atteinte au même tour par les deux n'est à personne.
    if (r[i] > 0 && (b[i] <= 0 || r[i] < b[i])) m.red.terr++;
    if (b[i] > 0 && (r[i] <= 0 || b[i] < r[i])) m.black.terr++;
  }
  return m;
}

export function analyse(steps: TStep[], game: { winner?: TColor; reason?: string }): TAnalysis {
  const series = (): TSeries => ({ mob: [], terr: [], reg: [] });
  const a: TAnalysis = { red: series(), black: series(), crit: new Map(), tip: null };
  for (const step of steps) {
    const m = measure(step.cells);
    for (const color of ['red', 'black'] as const) {
      a[color].mob.push(m[color].mob);
      a[color].terr.push(m[color].terr);
      a[color].reg.push(m[color].reg);
    }
  }
  const lead = (k: number) => a.red.terr[k] - a.black.terr[k];
  // L'ouverture est tirée au hasard : ses coups ne sont à aucune équipe.
  const swings = steps.flatMap((step, k) => {
    if (!step.move || step.opening) return [];
    const gain = (lead(k) - lead(k - 1)) * (step.cells[step.move.to] === RED ? 1 : -1);
    return Math.abs(gain) >= SWING ? [{ k, gain }] : [];
  });
  swings.sort((x, y) => Math.abs(y.gain) - Math.abs(x.gain));
  for (const s of swings.slice(0, CRITICAL)) a.crit.set(s.k, s.gain < 0 ? 'bad' : 'fine');

  // Seulement quand le perdant est bloqué : un temps dépassé ne dit rien du territoire.
  if (game.winner && game.reason === 'no_moves') {
    const w = a[game.winner].terr;
    const l = a[game.winner === 'red' ? 'black' : 'red'].terr;
    const first = steps.findIndex((s) => s.move && !s.opening) - 1;
    for (let k = steps.length - 2; k >= Math.max(0, first); k--) {
      if (l[k] >= w[k]) {
        a.tip = k + 1;
        break;
      }
    }
  }
  return a;
}

export type TDescription = { min: number; mean: number; median: number; max: number; sd: number; n: number };

export function describe(values: number[]): TDescription | null {
  if (!values.length) return null;
  const n = values.length;
  const sorted = [...values].sort((x, y) => x - y);
  const mean = values.reduce((s, v) => s + v, 0) / n;
  const median = n % 2 ? sorted[n >> 1] : (sorted[n / 2 - 1] + sorted[n / 2]) / 2;
  const sd = Math.sqrt(values.reduce((s, v) => s + (v - mean) ** 2, 0) / n);
  return { min: sorted[0], mean, median, max: sorted[n - 1], sd, n };
}
