import assert from 'node:assert/strict';
import { test } from 'node:test';

import { pointAt, rankSeries, smooth, topTeams } from '@/lib/bump';
import type { TRatingRow, TSide } from '@/types/games.types';

type TSmoothPoint = ReturnType<typeof smooth>[number];
type TTickSide = [team: string, rating: number, build?: string];
let clock = 0;

function must<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) {
    throw new Error('expected defined value');
  }
  return value;
}

function makeSide(team: string, rating: number, build?: string): TSide {
  const sideObj: TSide = {
    team,
    all: [rating, 0],
    session: [rating - 100, 0],
  };
  if (build !== undefined) {
    sideObj.build = build;
  }
  return sideObj;
}

// Ligne de timeline : `red` et `black` sont [équipe, rating après la partie, build].
function tick(
  [red, redRating, redBuild]: TTickSide,
  [black, blackRating, blackBuild]: TTickSide,
  session = 's1',
): TRatingRow {
  clock += 1;
  return {
    v: 1,
    id: `g${clock}`,
    ts: `2026-10-24T12:${String(clock).padStart(2, '0')}:00.000Z`,
    session,
    red: makeSide(red, redRating, redBuild),
    black: makeSide(black, blackRating, blackBuild),
  };
}

void test('top teams skip bots', () => {
  const ratings = [
    { team: 'a', build: 'x' },
    { team: 'bot' },
    { team: 'b', build: 'y' },
    { team: 'c', build: 'z' },
  ];
  assert.deepEqual(topTeams(ratings, 2), ['a', 'b']);
});

void test('ranks follow every game and branches start at the first game', () => {
  const rows = [
    tick(['a', 1520, 'a1'], ['b', 1480, 'b1']),
    tick(['c', 1540, 'c1'], ['b', 1460, 'b1']),
    tick(['b', 1500, 'b1'], ['a', 1500, 'a1']),
  ];
  const { columns, series } = rankSeries(rows, { selected: ['a', 'b', 'c'] });
  assert.equal(columns.length, 3);
  assert.deepEqual(
    must(series.get('a')).map((pt) => [pt.x, pt.rank]),
    [
      [0, 1],
      [1, 2],
      [2, 2],
    ],
  );
  // Égalité à 1500 : départagée par le nom, a avant b.
  assert.deepEqual(
    must(series.get('b')).map((pt) => [pt.x, pt.rank]),
    [
      [0, 2],
      [1, 3],
      [2, 3],
    ],
  );
  const cSeries = must(series.get('c'));
  assert.deepEqual(
    cSeries.map((pt) => [pt.x, pt.rank]),
    [[1, 1]],
  );
  assert.equal(pointAt(cSeries, 0), undefined);
  assert.equal(must(pointAt(cSeries, 2)).rank, 1);
});

void test('unchanged ranks add no point', () => {
  const rows = [
    tick(['a', 1520, 'a1'], ['b', 1480, 'b1']),
    tick(['c', 1510, 'c1'], ['d', 1490, 'd1']),
  ];
  const { series } = rankSeries(rows, { selected: ['a'] });
  assert.deepEqual(
    must(series.get('a')).map((pt) => pt.x),
    [0],
  );
});

void test('only chosen bots count in the ranking', () => {
  const rows = [tick(['a', 1520, 'a1'], ['n3', 1510]), tick(['b', 1480, 'b1'], ['n2', 1490])];
  const ranks = (selected: string[]): number[] => {
    const { series } = rankSeries(rows, { selected });
    return selected.map((team) => must(must(series.get(team)).at(-1)).rank);
  };
  assert.deepEqual(ranks(['a', 'b']), [1, 2]);
  assert.deepEqual(ranks(['a', 'b', 'n2']), [1, 3, 2]);
  assert.deepEqual(ranks(['a', 'b', 'n2', 'n3']), [1, 4, 3, 2]);
  const n3Series = rankSeries(rows, { selected: ['n3'] }).series.get('n3');
  assert.equal(must(must(n3Series).at(-1)).bot, true);
});

void test('a new build is a version point, even without a rank change', () => {
  const rows = [
    tick(['a', 1520, 'a1'], ['b', 1480, 'b1']),
    tick(['a', 1520, 'a2'], ['c', 1400, 'c1']),
    tick(['a', 1520, 'a2'], ['c', 1400, 'c1']),
  ];
  const { series } = rankSeries(rows, { selected: ['a'] });
  assert.deepEqual(
    must(series.get('a')).map((pt) => [pt.x, pt.version, pt.build]),
    [
      [0, false, 'a1'],
      [1, true, 'a2'],
    ],
  );
});

void test('session uses session ratings and only its games', () => {
  const rows = [
    tick(['a', 1700, 'a1'], ['b', 1300, 'b1'], 's1'),
    tick(['b', 1620, 'b1'], ['a', 1580, 'a1'], 's2'),
  ];
  const { columns, series } = rankSeries(rows, {
    key: 'session',
    session: 's2',
    selected: ['a', 'b'],
  });
  assert.deepEqual(
    columns.map((gameRow) => gameRow.id),
    [must(rows[1]).id],
  );
  assert.deepEqual(series.get('b'), [
    { x: 0, rank: 1, rating: 1520, build: 'b1', version: false, bot: false },
  ]);
});

void test('the window starts at the cutoff with the ratings reached before it', () => {
  const rows = [
    tick(['a', 1520, 'a1'], ['b', 1480, 'b1']),
    tick(['a', 1540, 'a1'], ['c', 1480, 'c1']),
    tick(['b', 1500, 'b1'], ['c', 1460, 'c1']),
  ];
  const { columns, series } = rankSeries(rows, {
    selected: ['b', 'c'],
    cutoff: Date.parse(must(rows[2]).ts),
  });
  assert.equal(columns.length, 1);
  assert.deepEqual(
    must(series.get('b')).map((pt) => [pt.x, pt.rank, pt.rating]),
    [[0, 2, 1500]],
  );
  assert.deepEqual(
    must(series.get('c')).map((pt) => [pt.x, pt.rank]),
    [[0, 3]],
  );
});

void test('smooth averages the rank over each bucket, from the start of the branch', () => {
  // Rang 2 de 5 à 9, rang 1 de 10 à 14, puis 2 à partir de 15 ; 20 colonnes en 4 paquets.
  const points: TSmoothPoint[] = [
    { x: 5, rank: 2 },
    { x: 10, rank: 1 },
    { x: 15, rank: 2 },
  ];
  assert.deepEqual(smooth(points, 20, 4), [
    { x: 7, rank: 2 },
    { x: 12, rank: 1 },
    { x: 17, rank: 2 },
  ]);
  const flips: TSmoothPoint[] = Array.from({ length: 12 }, (_, x) => ({ x, rank: (x % 2) + 1 }));
  assert.deepEqual(
    smooth(flips, 12, 3).map((pt) => pt.rank),
    [1.5, 1.5, 1.5],
  );
  assert.deepEqual(smooth(points, 20, 40), points);
});

void test('verified skips remote games and uses verified ratings', () => {
  const remote = tick(['a', 1530, 'a1'], ['r', 1470, 'r1']);
  remote.black.remote = true;
  const after = tick(['a', 1540, 'a1'], ['b', 1460, 'b1']);
  after.red.verified = { all: [1510, 10], session: [1410, 10] };
  const rows = [tick(['a', 1500, 'a1'], ['b', 1500, 'b1']), remote, after];
  const full = rankSeries(rows, { selected: ['a', 'r'] });
  assert.equal(full.columns.length, 3);
  const { columns, series } = rankSeries(rows, { selected: ['a', 'r'], verified: true });
  assert.deepEqual(
    columns.map((gameRow) => gameRow.id),
    [must(rows[0]).id, after.id],
  );
  const aPoints = must(series.get('a'));
  assert.equal(must(aPoints.at(-1)).rating, 1510);
  assert.deepEqual(series.get('r'), []);
});
