import assert from 'node:assert/strict';
import test from 'node:test';

import { correction, expectedMax, gamesPlayed, tournamentDuration } from './estimate.ts';

const near = (a, b) => assert.ok(Math.abs(a - b) < 1e-3, `${a} ≉ ${b}`);

test('le maximum de deux normales vaut 1/√π en moyenne', () => {
  near(expectedMax(1), 0);
  near(expectedMax(2), 1 / Math.sqrt(Math.PI));
  near(expectedMax(4), 1.0294);
});

test('un meilleur de 3 dure 2 ou 3 manches', () => {
  assert.deepEqual(gamesPlayed(2, 2), [2.5, 0.25]);
});

const match = (id, from, best_of = 3, head_start = [0, 0]) => ({
  id,
  from,
  best_of,
  head_start,
  score: head_start,
  teams: [null, null],
  status: 'waiting',
  games: [],
});
const four = (slots, multithread = []) => ({
  ts: '2026-11-08T17:00:00Z',
  game_ms: [60_000, 60_000],
  slots,
  entrants: multithread.map((team) => ({ team, multithread: true })),
  matches: [
    match('W1', [{ seed: 1 }, { seed: 4 }]),
    match('W2', [{ seed: 2 }, { seed: 3 }]),
    match('W3', [{ winner: 'W1' }, { winner: 'W2' }]),
    match('L1', [{ loser: 'W1' }, { loser: 'W2' }]),
    match('L2', [{ winner: 'L1' }, { loser: 'W3' }]),
    match('GF', [{ winner: 'W3' }, { winner: 'L2' }], 5, [1, 0]),
  ],
});
const [gf] = gamesPlayed(2, 3);
const parallel = (3 * 2.5 + gf) * 60_000 + 2 * expectedMax(2) * 60_000 * 0.5;

test('quatre équipes sur deux slots : quatre rondes dont deux en parallèle', () => {
  near(tournamentDuration(four([2, 1])), parallel);
});

test('sur un seul slot ordinaire, les séries se suivent', () => {
  near(tournamentDuration(four([1, 1])), (5 * 2.5 + gf) * 60_000);
});

test("une équipe multithread joue dans l'autre genre de slot, en parallèle", () => {
  const view = four([1, 1], ['mt']);
  view.matches[0].teams = ['mt', 'eq01'];
  near(tournamentDuration(view), (4 * 2.5 + gf) * 60_000 + expectedMax(2) * 60_000 * 0.5);
});

test('le temps restant oublie les matchs finis et les manches gagnées', () => {
  const view = four([2, 1]);
  for (const k of [0, 1]) {
    Object.assign(view.matches[k], { status: 'done', score: [2, 0], games: ['a', 'b'] });
  }
  Object.assign(view.matches[2], { status: 'running', score: [1, 0], games: ['c'] });
  const [w3] = gamesPlayed(1, 2);
  const rest = tournamentDuration(view);
  assert.ok(rest < tournamentDuration(view, true));
  near(tournamentDuration(view, true), parallel);
  near(rest, Math.max(w3, 2.5) * 60_000 + 0.5 * 60_000 * expectedMax(2) + (2.5 + gf) * 60_000);
});

test('la correction moyenne la durée réelle sur la durée prévue', () => {
  const ended = (ratio) => ({
    ...four([2, 1]),
    ended: new Date(Date.parse('2026-11-08T17:00:00Z') + ratio * parallel).toISOString(),
  });
  near(correction([ended(1.1), ended(1.3)]), 1.2);
  assert.equal(correction([four([2, 1]), { ...four([2, 1]), game_ms: [] }]), 1);
});
