// Durée prévue d'un tournoi : T ≈ Σ sur les rondes de (μ + σ·e_k), où une ronde regroupe les séries jouables
// en parallèle et e_k est l'espérance du maximum de k normales centrées réduites (e_2 = 1/√π). Une ronde qui a
// plus de séries que de slots de leur genre se joue en plusieurs vagues ; les deux genres roulent en même temps.

import type { TTournament } from '../types/tournament.types';

type TSeries = { mean: number; sd: number };

export function expectedMax(k: number) {
  const h = 0.01;
  let cdf = 0;
  let sum = 0;
  for (let x = -8; x < 8; x += h) {
    const pdf = Math.exp((-x * x) / 2) / Math.sqrt(2 * Math.PI);
    cdf += (pdf * h) / 2;
    sum += x * k * pdf * cdf ** (k - 1) * h;
    cdf += (pdf * h) / 2;
  }
  return sum;
}

export function gamesPlayed(a: number, b: number): [number, number] {
  if (!a || !b) {
    return [0, 0];
  }
  const [m1, v1] = gamesPlayed(a - 1, b);
  const [m2, v2] = gamesPlayed(a, b - 1);
  const mean = 1 + (m1 + m2) / 2;
  const square = 1 + m1 + m2 + (v1 + m1 ** 2 + v2 + m2 ** 2) / 2;
  return [mean, square - mean ** 2];
}

function wave(series: TSeries[]) {
  return (
    Math.max(...series.map((s) => s.mean)) +
    Math.max(...series.map((s) => s.sd)) * expectedMax(series.length)
  );
}

export function tournamentDuration(view: TTournament, fromStart = false) {
  const durations = view.game_ms ?? [];
  const { slots } = view;
  if (durations.length < 2 || !slots) {
    return null;
  }
  const multithread = new Set(view.entrants.filter((e) => e.multithread).map((e) => e.team));
  const m = durations.reduce((a, b) => a + b) / durations.length;
  const v = durations.reduce((a, d) => a + (d - m) ** 2, 0) / (durations.length - 1);
  const depth = new Map<string, number>();
  const rounds: [TSeries[], TSeries[]][] = [];
  for (const match of view.matches) {
    const before = Math.max(0, ...match.from.map((f) => depth.get((f.winner ?? f.loser)!) ?? 0));
    const bye = match.status === 'done' && match.games.length === 0;
    const over = bye || (!fromStart && match.status === 'done');
    depth.set(match.id, before + (over ? 0 : 1));
    if (over) {
      continue;
    }
    const score = fromStart ? match.head_start : match.score;
    const needed = Math.floor(match.best_of / 2) + 1;
    const [n, nVar] = gamesPlayed(needed - score[0], needed - score[1]);
    const wanted = Number(match.teams.some((t) => t !== null && multithread.has(t)));
    const kind = slots[wanted] ? wanted : 1 - wanted;
    (rounds[before] ??= [[], []])[kind].push({ mean: n * m, sd: Math.sqrt(n * v + nVar * m ** 2) });
  }
  let total = 0;
  for (const round of rounds.filter(Boolean)) {
    const queues = round.map((series, kind) => {
      let mean = 0;
      for (let i = 0; i < series.length; i += slots[kind]) {
        mean += wave(series.slice(i, i + slots[kind]));
      }
      return { mean, sd: Math.max(0, ...series.map((s) => s.sd)) };
    });
    total += wave(queues.filter((q) => q.mean));
  }
  return total;
}

export function correction(finished: TTournament[]) {
  const ratios = finished
    .map(
      (view) => (Date.parse(view.ended!) - Date.parse(view.ts)) / tournamentDuration(view, true)!,
    )
    .filter((r) => Number.isFinite(r) && r > 0);
  return ratios.length > 0 ? ratios.reduce((a, b) => a + b) / ratios.length : 1;
}
