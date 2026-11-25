import assert from 'node:assert/strict';
import { test } from 'node:test';

import { correction, expectedMax, gamesPlayed, tournamentDuration } from '@/lib/estimate';
import type { TMatch, TMatchSource, TTournament } from '@/types/tournament.types';

function must<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) {
    throw new Error('expected defined value');
  }
  return value;
}

function near(actual: number, expected: number): void {
  assert.ok(Math.abs(actual - expected) < 1e-3, `${actual} ≉ ${expected}`);
}

function match(
  id: string,
  from: TMatchSource[],
  bestOf = 3,
  headStart: [number, number] = [0, 0],
): TMatch {
  return {
    id,
    side: 'winners',
    round: 1,
    from,
    best_of: bestOf,
    head_start: headStart,
    score: headStart,
    teams: [null, null],
    status: 'pending',
    games: [],
  };
}

function four(slots: [number, number], multithread: string[] = []): TTournament {
  return {
    id: 'cup',
    ts: '2026-11-08T17:00:00Z',
    time_ms: 1000,
    game_ms: [60_000, 60_000],
    slots,
    entrants: multithread.map((team) => ({ team, rating: 1500, multithread: true })),
    matches: [
      match('W1', [{ seed: 1 }, { seed: 4 }]),
      match('W2', [{ seed: 2 }, { seed: 3 }]),
      match('W3', [{ winner: 'W1' }, { winner: 'W2' }]),
      match('L1', [{ loser: 'W1' }, { loser: 'W2' }]),
      match('L2', [{ winner: 'L1' }, { loser: 'W3' }]),
      match('GF', [{ winner: 'W3' }, { winner: 'L2' }], 5, [1, 0]),
    ],
  };
}

const [gf] = gamesPlayed(2, 3);
const parallel = (3 * 2.5 + gf) * 60_000 + 2 * expectedMax(2) * 60_000 * 0.5;

function ended(ratio: number): TTournament {
  return {
    ...four([2, 1]),
    ended: new Date(Date.parse('2026-11-08T17:00:00Z') + ratio * parallel).toISOString(),
  };
}

void test('le maximum de deux normales vaut 1/√π en moyenne', () => {
  near(expectedMax(1), 0);
  near(expectedMax(2), 1 / Math.sqrt(Math.PI));
  near(expectedMax(4), 1.0294);
});

void test('un meilleur de 3 dure 2 ou 3 manches', () => {
  assert.deepEqual(gamesPlayed(2, 2), [2.5, 0.25]);
});

void test('quatre équipes sur deux slots : quatre rondes dont deux en parallèle', () => {
  const duration = must(tournamentDuration(four([2, 1])));
  near(duration, parallel);
});

void test('sur un seul slot ordinaire, les séries se suivent', () => {
  const duration = must(tournamentDuration(four([1, 1])));
  near(duration, (5 * 2.5 + gf) * 60_000);
});

void test("une équipe multithread joue dans l'autre genre de slot, en parallèle", () => {
  const view = four([1, 1], ['mt']);
  must(view.matches[0]).teams = ['mt', 'eq01'];
  near(must(tournamentDuration(view)), (4 * 2.5 + gf) * 60_000 + expectedMax(2) * 60_000 * 0.5);
});

void test('le temps restant oublie les matchs finis et les manches gagnées', () => {
  const view = four([2, 1]);
  for (const k of [0, 1]) {
    Object.assign(must(view.matches[k]), { status: 'done', score: [2, 0], games: ['a', 'b'] });
  }
  Object.assign(must(view.matches[2]), { status: 'running', score: [1, 0], games: ['c'] });
  const [w3] = gamesPlayed(1, 2);
  const rest = must(tournamentDuration(view));
  const total = must(tournamentDuration(view, true));
  assert.ok(rest < total);
  near(total, parallel);
  near(rest, Math.max(w3, 2.5) * 60_000 + 0.5 * 60_000 * expectedMax(2) + (2.5 + gf) * 60_000);
});

void test('la correction moyenne la durée réelle sur la durée prévue', () => {
  near(correction([ended(1.1), ended(1.3)]), 1.2);
  assert.equal(correction([four([2, 1]), { ...four([2, 1]), game_ms: [] }]), 1);
});
