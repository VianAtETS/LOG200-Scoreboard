import assert from 'node:assert/strict';
import { test } from 'node:test';
import { BETWEEN, gamesOf, isRemote, parseJsonl, ratingOf, record } from '@/lib/elo';
import { BLACK, DESTROYED, EMPTY, RED, positions, squareIndex, squareName } from '@/lib/replay';
import type { TColor, TGameRow, TSide } from '@/types/games.types';

let clock = 0;

function must<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) {
    throw new Error('expected defined value');
  }
  return value;
}

function makeSide(team: string, build?: string, remote?: boolean): TSide {
  const sideObj: TSide = { team, all: [1500, 0], session: [1500, 0] };
  if (build !== undefined) {
    sideObj.build = build;
  }
  if (remote !== undefined) {
    sideObj.remote = remote;
  }
  return sideObj;
}

function game(red: string, black: string, winner: TColor, extra: Partial<TGameRow> = {}): TGameRow {
  clock += 1;
  return {
    v: 1,
    id: `g${clock}`,
    session: 's1',
    ts: `2026-10-24T12:00:${String(clock).padStart(2, '0')}.000Z`,
    red: makeSide(red),
    black: makeSide(black),
    winner,
    reason: 'no_moves',
    moves: 0,
    duration_ms: 0,
    ...extra,
  };
}

function ids(games: readonly TGameRow[]): string[] {
  return games.map((gameRow) => gameRow.id);
}

void test('parseJsonl skips truncated lines and unknown versions', () => {
  const line = JSON.stringify(game('a', 'b', 'red'));
  const { games, skipped } = parseJsonl(`${line}\n{"v":2}\n${line.slice(0, 20)}`);
  assert.equal(games.length, 1);
  assert.equal(skipped, 2);
});

void test('replay squares follow server indices', () => {
  assert.equal(squareIndex('G', 1), 138);
  assert.equal(squareIndex('F', 12), 5);
  assert.equal(squareName(138), 'G1');
  const steps = positions({ opening: ['G1-G4-D7'], moves: ['F12-F9-G2'] });
  assert.equal(steps.length, 3);
  const last = must(steps[2]).cells;
  assert.equal(last[squareIndex('G', 4)], RED);
  assert.equal(last[squareIndex('F', 9)], BLACK);
  assert.equal(last[squareIndex('G', 1)], EMPTY);
  assert.equal(last[squareIndex('D', 7)], DESTROYED);
  assert.equal(last[squareIndex('G', 2)], DESTROYED);
  assert.ok(must(steps[1]).opening && !must(steps[2]).opening);
});

void test('gamesOf merges team files once each, in game order, with both modes', () => {
  const ab = game('a', 'b', 'red');
  const ac = game('a', 'c', 'black');
  const bc = game('b', 'c', 'red', { session: 's2' });
  const cd = game('c', 'd', 'red');
  const files = new Map<string, TGameRow[]>([
    ['a', [ab, ac]],
    ['b', [ab, bc]],
    ['c', [ac, bc, cd]],
  ]);
  assert.deepEqual(ids(gamesOf(files, ['b', 'a'])), ids([ab, ac, bc]));
  assert.deepEqual(ids(gamesOf(files, ['a', 'b'], { mode: BETWEEN })), ids([ab]));
  assert.deepEqual(ids(gamesOf(files, ['a', 'b', 'c'], { mode: BETWEEN })), ids([ab, ac, bc]));
  assert.deepEqual(ids(gamesOf(files, ['b', 'c'], { mode: BETWEEN, session: 's2' })), ids([bc]));
  assert.deepEqual(ids(gamesOf(files, ['c'], { mode: BETWEEN })), ids([ac, bc, cd]));
  assert.deepEqual(gamesOf(files, []), []);
});

void test('ratingOf reads verified ratings only when asked and present', () => {
  const side: TSide = {
    team: 'a',
    all: [1520, 20],
    session: [1510, 10],
    verified: { all: [1500, 0], session: [1490, -10] },
  };
  const unverifiedSide: TSide = { team: 'a', all: [1520, 20], session: [1510, 10] };
  assert.deepEqual(ratingOf(side, 'all'), [1520, 20]);
  assert.deepEqual(ratingOf(side, 'session', true), [1490, -10]);
  assert.deepEqual(ratingOf(unverifiedSide, 'all', true), [1520, 20]);
});

void test('record counts the current build against every version of the opponent', () => {
  const games = [
    game('a', 'b', 'red', { red: makeSide('a', 'old'), black: makeSide('b', 'x') }),
    game('b', 'a', 'black', { red: makeSide('b', 'x'), black: makeSide('a', 'new') }),
    game('a', 'b', 'black', { red: makeSide('a', 'new'), black: makeSide('b', 'y') }),
    game('a', 'c', 'red', { red: makeSide('a', 'new') }),
  ];
  assert.deepEqual(record(games, 'a', 'b', 'new'), { wins: 1, games: 2 });
  assert.deepEqual(record(games, 'b', 'a', 'x'), { wins: 0, games: 2 });
  assert.deepEqual(record(games, 'c', 'a'), { wins: 0, games: 1 });
  assert.deepEqual(record(games, 'a', 'd', 'new'), { wins: 0, games: 0 });
});

void test('isRemote is true when either side played remotely', () => {
  assert.equal(isRemote(game('a', 'b', 'red')), false);
  const remoteSide = makeSide('b', undefined, true);
  assert.equal(isRemote(game('a', 'b', 'red', { black: remoteSide })), true);
});
