// Statistiques d'un duel sur ses paires complètes, portées de l'arène isola de Justin Pagès
// (gitlab.com/justin-pages/isola, arena/web/app.js) : test du signe, SPRT, intervalle de Wilson, Elo.
// Les deux parties d'une paire partagent leur ouverture : seules les paires 2-0 départagent.

import type { TDuelPair } from '@/types/games.types';

const HALF = 0.5;
const WILSON_FACTOR = 4;
const ELO_SCALE = 400;

export type TPairCounts = { a: number; b: number; split: number };

// Paires gagnées 2-0 par `a`, par l'autre équipe, et paires 1-1.
export function pairCounts(pairs: TDuelPair[], team: string): TPairCounts {
  const counts = { a: 0, b: 0, split: 0 };
  for (const [first, second] of pairs.map((pair) => pair.winners)) {
    if (first !== second) {
      counts.split += 1;
    } else if (first === team) {
      counts.a += 1;
    } else {
      counts.b += 1;
    }
  }
  return counts;
}

// Test du signe bilatéral, en logarithmes : 0,5^n vaut 0 en flottant dès n = 1075. NaN sans paire.
export function signTest(left: number, right: number): number {
  const count = left + right;
  if (!count) {
    return Number.NaN;
  }
  const k = Math.max(left, right);
  const half = count * Math.log(HALF);
  // log C(n, i)
  let logChoose = 0;
  let tail = 0;
  for (let i = 0; i <= count; i++) {
    if (i > 0) {
      logChoose += Math.log(count - i + 1) - Math.log(i);
    }
    if (i >= k) {
      tail += Math.exp(logChoose + half);
    }
  }
  return Math.min(1, 2 * tail);
}

// H0 : chaque côté gagne la moitié des paires 2-0 ; H1 : 65 %. α = 2,5 % par côté, β = 10 %.
const SPRT_P = 0.65;
const SPRT_ALPHA = 0.025;
const SPRT_BETA = 0.1;
const UPPER = Math.log((1 - SPRT_BETA) / SPRT_ALPHA);
const LOWER = Math.log(SPRT_BETA / (1 - SPRT_ALPHA));
const WIN = Math.log(SPRT_P / HALF);
const LOSS = Math.log((1 - SPRT_P) / HALF);

// Paires 2-0 attendues avant un verdict quand un côté est meilleur (H1) : environ 78.
export const SPRT_EXPECTED = Math.round(UPPER / (SPRT_P * WIN + (1 - SPRT_P) * LOSS));

export type TSprt = 'a' | 'b' | 'close' | 'undecided';

// SPRT sur les paires 2-0 : `a` ou `b` meilleur, `close` sans grand écart, ou pas encore de verdict.
export function sprt(left: number, right: number): TSprt {
  const forA = left * WIN + right * LOSS;
  const forB = right * WIN + left * LOSS;
  if (forA >= UPPER) {
    return 'a';
  }
  if (forB >= UPPER) {
    return 'b';
  }
  return forA <= LOWER && forB <= LOWER ? 'close' : 'undecided';
}

// Intervalle de Wilson à 95 % de k succès sur n.
export function wilson(successes: number, count: number): [number, number] {
  if (!count) {
    return [Number.NaN, Number.NaN];
  }
  const zScore = 1.96;
  const proportion = successes / count;
  const denominator = 1 + (zScore * zScore) / count;
  const center = (proportion + (zScore * zScore) / (2 * count)) / denominator;
  const margin =
    (zScore *
      Math.sqrt(
        (proportion * (1 - proportion)) / count +
          (zScore * zScore) / (WILSON_FACTOR * count * count),
      )) /
    denominator;
  return [center - margin, center + margin];
}

// Écart Elo qui donne ce score ; NaN à 0 ou 1.
export const eloGap = (score: number): number =>
  score > 0 && score < 1 ? -ELO_SCALE * Math.log10(1 / score - 1) : Number.NaN;
