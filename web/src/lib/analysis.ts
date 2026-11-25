// Analyse d'une partie, portée de l'arène isola de Justin Pagès (gitlab.com/justin-pages/isola, arena/web/core.js
// et app.js) : mobilité, territoire et région de chaque couleur après chaque coup, coups critiques et point de bascule.

import type { TColor } from '@/types/games.types';
import { BLACK, EMPTY, RED, SIZE } from './replay.ts';
import type { TStep } from './replay.ts';

export type TSeries = { mob: number[]; terr: number[]; reg: number[] };

export type TAnalysis = {
  red: TSeries;
  black: TSeries;
  // Coup -> 'fine' s'il a gagné du territoire à son auteur, 'bad' s'il lui en a coûté.
  crit: Map<number, 'fine' | 'bad'>;
  // Coup après lequel le perdant n'a plus jamais repris l'avantage en territoire.
  tip: number | null;
};

const DIRS = [
  [-1, -1],
  [0, -1],
  [1, -1],
  [-1, 0],
  [1, 0],
  [-1, 1],
  [0, 1],
  [1, 1],
] as const;
// Écart de territoire minimal d'un coup critique, et leur nombre maximal.
const SWING = 8;
const CRITICAL = 8;
const MAX_MOVE_DISTANCE = 3;
const series = (): TSeries => ({ mob: [], terr: [], reg: [] });
const at = (values: number[], index: number): number => values[index] ?? Number.NaN;

// Nombre de tours pour atteindre chaque case (-1 : inaccessible), un tour = jusqu'à 3 cases en ligne droite.
function distances(cells: number[], pawn: number): number[] {
  const dist = Array.from<number>({ length: SIZE * SIZE }).fill(-1);
  const start = cells.indexOf(pawn);
  const queue = [start];
  dist[start] = 0;
  for (let h = 0; h < queue.length; h++) {
    const cur = at(queue, h);
    for (const [dx, dy] of DIRS) {
      for (
        let stride = 1, x = (cur % SIZE) + dx, y = Math.floor(cur / SIZE) + dy;
        stride <= MAX_MOVE_DISTANCE;
        stride++, x += dx, y += dy
      ) {
        if (x < 0 || x >= SIZE || y < 0 || y >= SIZE || cells[y * SIZE + x] !== EMPTY) {
          break;
        }
        const i = y * SIZE + x;
        if (at(dist, i) < 0) {
          dist[i] = at(dist, cur) + 1;
          queue.push(i);
        }
      }
    }
  }
  return dist;
}

// Mobilité (cases à un tour), région (cases accessibles) et territoire (cases atteintes avant l'adversaire).
function measure(cells: number[]): Record<TColor, Record<keyof TSeries, number>> {
  const redDistances = distances(cells, RED);
  const blackDistances = distances(cells, BLACK);
  const metrics = { red: { mob: 0, terr: 0, reg: 0 }, black: { mob: 0, terr: 0, reg: 0 } };
  for (let i = 0; i < SIZE * SIZE; i++) {
    if (at(redDistances, i) === 1) {
      metrics.red.mob += 1;
    }
    if (at(blackDistances, i) === 1) {
      metrics.black.mob += 1;
    }
    if (at(redDistances, i) > 0) {
      metrics.red.reg += 1;
    }
    if (at(blackDistances, i) > 0) {
      metrics.black.reg += 1;
    }
    // Une case atteinte au même tour par les deux n'est à personne.
    if (
      at(redDistances, i) > 0 &&
      (at(blackDistances, i) <= 0 || at(redDistances, i) < at(blackDistances, i))
    ) {
      metrics.red.terr += 1;
    }
    if (
      at(blackDistances, i) > 0 &&
      (at(redDistances, i) <= 0 || at(blackDistances, i) < at(redDistances, i))
    ) {
      metrics.black.terr += 1;
    }
  }
  return metrics;
}

export function analyse(steps: TStep[], game: { winner?: TColor; reason?: string }): TAnalysis {
  const analysis: TAnalysis = { red: series(), black: series(), crit: new Map(), tip: null };
  for (const step of steps) {
    const metrics = measure(step.cells);
    for (const color of ['red', 'black'] as const) {
      analysis[color].mob.push(metrics[color].mob);
      analysis[color].terr.push(metrics[color].terr);
      analysis[color].reg.push(metrics[color].reg);
    }
  }
  const lead = (k: number): number => at(analysis.red.terr, k) - at(analysis.black.terr, k);
  // L'ouverture est tirée au hasard : ses coups ne sont à aucune équipe.
  const swings = steps.flatMap((step, k) => {
    if (!step.move || step.opening) {
      return [];
    }
    const gain = (lead(k) - lead(k - 1)) * (step.cells[step.move.to] === RED ? 1 : -1);
    return Math.abs(gain) >= SWING ? [{ k, gain }] : [];
  });
  swings.sort((x, y) => Math.abs(y.gain) - Math.abs(x.gain));
  for (const swing of swings.slice(0, CRITICAL)) {
    analysis.crit.set(swing.k, swing.gain < 0 ? 'bad' : 'fine');
  }

  // Seulement quand le perdant est bloqué : un temps dépassé ne dit rien du territoire.
  if (game.winner && game.reason === 'no_moves') {
    const winner = analysis[game.winner].terr;
    const loser = analysis[game.winner === 'red' ? 'black' : 'red'].terr;
    const first = steps.findIndex((step) => step.move && !step.opening) - 1;
    for (let k = steps.length - 2; k >= Math.max(0, first); k--) {
      if (at(loser, k) >= at(winner, k)) {
        analysis.tip = k + 1;
        break;
      }
    }
  }
  return analysis;
}

export type TDescription = {
  min: number;
  mean: number;
  median: number;
  max: number;
  sd: number;
  n: number;
};

export function describe(values: number[]): TDescription | null {
  if (values.length === 0) {
    return null;
  }
  const count = values.length;
  const sorted = values.toSorted((x, y) => x - y);
  const mean = values.reduce((sum, value) => sum + value, 0) / count;
  const median =
    count % 2
      ? at(sorted, Math.floor(count / 2))
      : (at(sorted, count / 2 - 1) + at(sorted, count / 2)) / 2;
  const sd = Math.sqrt(values.reduce((sum, value) => sum + (value - mean) ** 2, 0) / count);
  return { min: at(sorted, 0), mean, median, max: at(sorted, count - 1), sd, n: count };
}
