// Durée prévue d'un tournoi : T ≈ Σ sur les rondes de (μ + σ·e_k), où une ronde regroupe les séries jouables
// en parallèle et e_k est l'espérance du maximum de k normales centrées réduites (e_2 = 1/√π). Une ronde qui a
// plus de séries que de slots de leur genre se joue en plusieurs vagues ; les deux genres roulent en même temps.

import type { TTournament } from '@/types/tournament.types';

type TSeries = { mean: number; sd: number };

type TRound = [TSeries[], TSeries[]];
const NORMAL_STEP = 0.01;
const NORMAL_BOUND = 8;

export function expectedMax(k: number): number {
  const h = NORMAL_STEP;
  let cdf = 0;
  let sum = 0;
  for (let x = -NORMAL_BOUND; x < NORMAL_BOUND; x += h) {
    const pdf = Math.exp((-x * x) / 2) / Math.sqrt(2 * Math.PI);
    cdf += (pdf * h) / 2;
    sum += x * k * pdf * cdf ** (k - 1) * h;
    cdf += (pdf * h) / 2;
  }
  return sum;
}

export function gamesPlayed(leftWinsNeeded: number, rightWinsNeeded: number): [number, number] {
  if (!leftWinsNeeded || !rightWinsNeeded) {
    return [0, 0];
  }
  const [m1, v1] = gamesPlayed(leftWinsNeeded - 1, rightWinsNeeded);
  const [m2, v2] = gamesPlayed(leftWinsNeeded, rightWinsNeeded - 1);
  const mean = 1 + (m1 + m2) / 2;
  const square = 1 + m1 + m2 + (v1 + m1 ** 2 + v2 + m2 ** 2) / 2;
  return [mean, square - mean ** 2];
}

function wave(series: TSeries[]): number {
  return (
    Math.max(...series.map((item) => item.mean)) +
    Math.max(...series.map((item) => item.sd)) * expectedMax(series.length)
  );
}

function queueWave(series: TSeries[], slotCount: number): TSeries {
  let mean = 0;
  for (let i = 0; i < series.length; i += slotCount) {
    mean += wave(series.slice(i, i + slotCount));
  }
  return { mean, sd: Math.max(0, ...series.map((item) => item.sd)) };
}

function seriesByRound(
  view: TTournament,
  slots: [number, number],
  meanDuration: number,
  variance: number,
  fromStart: boolean,
): (TRound | undefined)[] {
  const multithread = new Set(
    view.entrants.flatMap((entrant) => (entrant.multithread ? [entrant.team] : [])),
  );
  const depth = new Map<string, number>();
  const rounds: (TRound | undefined)[] = [];
  for (const match of view.matches) {
    const before = Math.max(
      0,
      ...match.from.map((from) => {
        const ref = from.winner ?? from.loser;
        return ref === undefined ? 0 : (depth.get(ref) ?? 0);
      }),
    );
    const over = match.status === 'done' && (!fromStart || match.games.length === 0);
    depth.set(match.id, before + (over ? 0 : 1));
    if (over) {
      continue;
    }
    const score = fromStart ? match.head_start : match.score;
    const needed = Math.floor(match.best_of / 2) + 1;
    const [gamesMean, gamesVar] = gamesPlayed(needed - score[0], needed - score[1]);
    const wanted: 0 | 1 = match.teams.some((team) => team !== null && multithread.has(team))
      ? 1
      : 0;
    const otherKind: 0 | 1 = wanted === 0 ? 1 : 0;
    const kind: 0 | 1 = slots[wanted] ? wanted : otherKind;
    rounds[before] ??= [[], []];
    rounds[before][kind].push({
      mean: gamesMean * meanDuration,
      sd: Math.sqrt(gamesMean * variance + gamesVar * meanDuration ** 2),
    });
  }
  return rounds;
}

export function tournamentDuration(view: TTournament, fromStart = false): number | null {
  const durations = view.game_ms ?? [];
  const { slots } = view;
  if (durations.length < 2 || !slots) {
    return null;
  }
  const mean = durations.reduce((acc, duration) => acc + duration, 0) / durations.length;
  const variance =
    durations.reduce((acc, duration) => acc + (duration - mean) ** 2, 0) / (durations.length - 1);
  let total = 0;
  for (const round of seriesByRound(view, slots, mean, variance, fromStart)) {
    if (!round) {
      continue;
    }
    const queues = [queueWave(round[0], slots[0]), queueWave(round[1], slots[1])].filter((queue) =>
      Boolean(queue.mean),
    );
    total += wave(queues);
  }
  return total;
}

export function correction(finished: TTournament[]): number {
  const ratios = finished.flatMap((view) => {
    const ratio =
      (Date.parse(view.ended ?? '') - Date.parse(view.ts)) / (tournamentDuration(view, true) ?? 0);
    return Number.isFinite(ratio) && ratio > 0 ? [ratio] : [];
  });
  return ratios.length > 0 ? ratios.reduce((sum, ratio) => sum + ratio) / ratios.length : 1;
}
