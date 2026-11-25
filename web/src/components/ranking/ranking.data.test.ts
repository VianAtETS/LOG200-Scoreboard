import { test } from 'node:test';
import assert from 'node:assert/strict';

import type { TRankingRow } from '../../lib/archives.ts';
import { DEFAULT_STATS, rankingView } from './ranking.data.ts';
import type { TRankingSort, TSortColumn } from './ranking.data.ts';

const ratings: TRankingRow[] = [
  {
    team: 'a',
    build: 'a1',
    rating: 1600,
    games: 20,
    wins: 8,
    losses: 12,
    build_games: 10,
    build_wins: 4,
    pace: { mean: 10, p50: 10, p95: 20, p99: 25, max: 30, count: 100, limit: 500 },
  },
  {
    team: 'a',
    build: 'a0',
    rating: 1550,
    archived: true,
    games: 6,
    wins: 3,
    losses: 3,
    build_games: 6,
    build_wins: 3,
  },
  {
    team: 'b',
    build: 'b1',
    rating: 1500,
    games: 12,
    wins: 8,
    losses: 4,
    build_games: 8,
    build_wins: 4,
    pace: { mean: 5, p50: 5, p95: 10, p99: 15, max: 20, count: 100, limit: 500 },
  },
  {
    team: 'c',
    build: 'c1',
    rating: 1400,
    games: 0,
    wins: 0,
    losses: 0,
    build_games: 0,
    build_wins: 0,
  },
];

const view = (column: TSortColumn, direction: TRankingSort['direction']) =>
  rankingView(ratings, { column, direction }, DEFAULT_STATS);
const builds = (rows: readonly TRankingRow[]) => rows.map((r) => r.build);

test('equal wins are ordered by fewer losses in descending order, reversed in ascending order', () => {
  assert.deepEqual(builds(view('record', 'descending').rows), ['b1', 'a1', 'a0', 'c1']);
  assert.deepEqual(builds(view('record', 'ascending').rows), ['c1', 'a0', 'a1', 'b1']);
});

test('a team without current-build games has no win rate and stays last in either direction', () => {
  assert.deepEqual(builds(view('pct', 'descending').rows), ['a0', 'b1', 'a1', 'c1']);
  assert.deepEqual(builds(view('pct', 'ascending').rows), ['a1', 'a0', 'b1', 'c1']);
});

test('missing move times stay last and equal values retain the official input order', () => {
  assert.deepEqual(builds(view('pace', 'descending').rows), ['a1', 'b1', 'a0', 'c1']);
  assert.deepEqual(builds(view('pace', 'ascending').rows), ['b1', 'a1', 'a0', 'c1']);
});

test('official ranks ignore archives and do not change with display order', () => {
  const ascending = view('rank', 'ascending');
  const descending = view('rank', 'descending');
  assert.deepEqual(
    [...ascending.ranks],
    [
      ['a', 1],
      ['b', 2],
      ['c', 3],
    ],
  );
  assert.deepEqual([...descending.ranks], [...ascending.ranks]);
  assert.deepEqual(builds(ascending.rows), ['a1', 'b1', 'c1', 'a0']);
  assert.deepEqual(builds(descending.rows), ['c1', 'b1', 'a1', 'a0']);
  assert.deepEqual(builds(ratings), ['a1', 'a0', 'b1', 'c1']);
});
