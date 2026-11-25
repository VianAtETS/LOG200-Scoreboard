// Statistiques d'un duel sur ses paires complètes, portées de l'arène isola de Justin Pagès
// (gitlab.com/justin-pages/isola, arena/web/app.js) : test du signe, SPRT, intervalle de Wilson, Elo.
// Les deux parties d'une paire partagent leur ouverture : seules les paires 2-0 départagent.

import type { TDuelPair } from '../types/games.types';

export type TPairCounts = { a: number; b: number; split: number };

// Paires gagnées 2-0 par `a`, par l'autre équipe, et paires 1-1.
export function pairCounts(pairs: TDuelPair[], a: string): TPairCounts {
  const counts = { a: 0, b: 0, split: 0 };
  for (const [first, second] of pairs.map((p) => p.winners)) {
    if (first !== second) {
      counts.split++;
    } else if (first === a) {
      counts.a++;
    } else {
      counts.b++;
    }
  }
  return counts;
}

// Test du signe bilatéral, en logarithmes : 0,5^n vaut 0 en flottant dès n = 1075. NaN sans paire.
export function signTest(a: number, b: number): number {
  const n = a + b;
  if (!n) {
    return NaN;
  }
  const k = Math.max(a, b);
  const half = n * Math.log(0.5);
  let logChoose = 0; // log C(n, i)
  let tail = 0;
  for (let i = 0; i <= n; i++) {
    if (i > 0) {
      logChoose += Math.log(n - i + 1) - Math.log(i);
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
const WIN = Math.log(SPRT_P / 0.5);
const LOSS = Math.log((1 - SPRT_P) / 0.5);

// Paires 2-0 attendues avant un verdict quand un côté est meilleur (H1) : environ 78.
export const SPRT_EXPECTED = Math.round(UPPER / (SPRT_P * WIN + (1 - SPRT_P) * LOSS));

export type TSprt = 'a' | 'b' | 'close' | 'undecided';

// SPRT sur les paires 2-0 : `a` ou `b` meilleur, `close` sans grand écart, ou pas encore de verdict.
export function sprt(a: number, b: number): TSprt {
  const forA = a * WIN + b * LOSS;
  const forB = b * WIN + a * LOSS;
  if (forA >= UPPER) {
    return 'a';
  }
  if (forB >= UPPER) {
    return 'b';
  }
  return forA <= LOWER && forB <= LOWER ? 'close' : 'undecided';
}

// Intervalle de Wilson à 95 % de k succès sur n.
export function wilson(k: number, n: number): [number, number] {
  if (!n) {
    return [NaN, NaN];
  }
  const z = 1.96;
  const p = k / n;
  const d = 1 + (z * z) / n;
  const c = (p + (z * z) / (2 * n)) / d;
  const h = (z * Math.sqrt((p * (1 - p)) / n + (z * z) / (4 * n * n))) / d;
  return [c - h, c + h];
}

// Écart Elo qui donne ce score ; NaN à 0 ou 1.
export const eloGap = (score: number) =>
  score > 0 && score < 1 ? -400 * Math.log10(1 / score - 1) : Number.NaN;
