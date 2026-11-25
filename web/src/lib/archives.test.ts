import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  archiveKey,
  archiveOptions,
  archivedRows,
  currentBuilds,
  withArchivedRows,
} from '@/lib/archives';
import type { TRankingRow } from '@/lib/archives';
import type { TGameRow } from '@/types/games.types';
import type { TPace, TScope, TSummary, TTeamRating } from '@/types/summary.types';

function must<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) {
    throw new Error('expected defined value');
  }
  return value;
}

function teamRating(overrides: Partial<TTeamRating> & Pick<TTeamRating, 'team'>): TTeamRating {
  return {
    rating: 1500,
    games: 0,
    wins: 0,
    losses: 0,
    build_wins: 0,
    build_games: 0,
    ...overrides,
  };
}

function rankingRow(overrides: Partial<TRankingRow> & Pick<TRankingRow, 'team'>): TRankingRow {
  return {
    rating: 1500,
    games: 0,
    wins: 0,
    losses: 0,
    build_wins: 0,
    build_games: 0,
    ...overrides,
  };
}

function scopeOf(ratings: TTeamRating[]): TScope {
  return { ratings, recent: [], games: 0 };
}

function summary(): TSummary {
  return {
    sessions: ['s1'],
    builds: { eq110: ['same', 'new110'], eq118: ['same', 'new118'] },
    scopes: {
      '': scopeOf([
        teamRating({ team: 'eq110', build: 'new110', last: '2' }),
        teamRating({ team: 'eq118', build: 'new118', last: '2' }),
      ]),
    },
  };
}

function game(id: string, build: string, rating: number, extra: Partial<TGameRow> = {}): TGameRow {
  return {
    v: 1,
    id,
    ts: id,
    session: 's1',
    winner: 'red',
    reason: 'no_moves',
    moves: 10,
    duration_ms: 1000,
    red: { team: 'eq118', build, all: [rating, 20], session: [rating - 100, 10] },
    black: { team: 'bot', all: [1500, 0], session: [1500, 0] },
    ...extra,
  };
}

void test('archives are identified by team and build, with stable global vN labels', () => {
  const options = archiveOptions(summary());
  assert.deepEqual(
    options.map((opt) => [opt.team, opt.version, opt.build]),
    [
      ['eq110', 1, 'same'],
      ['eq118', 1, 'same'],
    ],
  );
  assert.notEqual(must(options[0]).key, must(options[1]).key);
  assert.equal(archiveKey('team,unsafe', 'x'), 'team%2Cunsafe@x');
});

void test('a restored build becomes current again without renumbering old versions', () => {
  const summaryVal = summary();
  summaryVal.scopes['s2'] = scopeOf([teamRating({ team: 'eq118', build: 'same', last: '3' })]);
  assert.equal(currentBuilds(summaryVal).get('eq118'), 'same');
  const eq118Options = archiveOptions(summaryVal).filter((opt) => opt.team === 'eq118');
  assert.deepEqual(
    eq118Options.map((opt) => [opt.version, opt.build]),
    [[2, 'new118']],
  );
});

void test('archived statistics stop at that hash and do not mutate the active ranking', () => {
  const summaryVal = summary();
  const before = JSON.stringify(summaryVal);
  const selected = archiveOptions(summaryVal).filter((opt) => opt.team === 'eq118');
  const result = archivedRows(
    [
      game('1', 'same', 1520),
      game('2', 'same', 1500, { winner: 'black' }),
      game('3', 'new118', 1700),
    ],
    selected,
  );
  assert.equal(result.length, 1);
  const first = must(result[0]);
  assert.deepEqual([first.rating, first.peak, first.games, first.wins], [1500, 1520, 2, 1]);
  assert.equal(first.archived, true);
  assert.equal(JSON.stringify(summaryVal), before);
  assert.deepEqual(archivedRows([game('1', 'same', 1520)], []), []);
});

void test('session, tournament and verified filters use the same conventions as the active view', () => {
  const selected = archiveOptions(summary()).filter((opt) => opt.team === 'eq118');
  const rows: TGameRow[] = [
    game('1', 'same', 1520),
    game('2', 'same', 1540, { session: 's2' }),
    game('3', 'same', 1600, { tournament: { id: 'cup', match: 'W1', game: 1 } }),
    game('4', 'same', 1700, {
      black: { team: 'remote', remote: true, all: [1500, 0], session: [1500, 0] },
    }),
  ];
  assert.equal(must(archivedRows(rows, selected)[0]).games, 3);
  assert.equal(must(archivedRows(rows, selected, { session: 's1' })[0]).games, 3);
  assert.equal(must(archivedRows(rows, selected, { verified: true })[0]).games, 2);
  assert.equal(must(archivedRows(rows, selected, { session: 's2' })[0]).rating, 1440);
});

void test('selected hashes cannot leak another team that used the same bytes', () => {
  const selected = archiveOptions(summary()).filter((opt) => opt.team === 'eq110');
  assert.deepEqual(archivedRows([game('1', 'same', 1520)], selected), []);
});

void test('several archived versions of the same team can be selected independently', () => {
  const summaryVal = summary();
  must(summaryVal.builds['eq118']).push('third');
  must(summaryVal.scopes['']).ratings[1] = teamRating({ team: 'eq118', build: 'third', last: '4' });
  const options = archiveOptions(summaryVal).filter((opt) => opt.team === 'eq118');
  assert.deepEqual(
    options.map((opt) => opt.version),
    [1, 2],
  );
  const rows = [game('1', 'same', 1520), game('2', 'new118', 1540), game('3', 'third', 1560)];
  const both = archivedRows(rows, options);
  assert.deepEqual(
    both.map((row) => [row.build, row.games, row.rating]),
    [
      ['same', 1, 1520],
      ['new118', 1, 1540],
    ],
  );
  assert.deepEqual(
    archivedRows(rows, [must(options[1])]).map((row) => row.build),
    ['new118'],
  );
});

void test('selected archive rows take their Elo place, after an active row at the same Elo', () => {
  const active: TTeamRating[] = [
    teamRating({ team: 'eq118', build: 'new', rating: 1600 }),
    teamRating({ team: 'eq110', build: 'new', rating: 1500 }),
  ];
  const old: TRankingRow[] = [
    rankingRow({ team: 'eq110', build: 'old', rating: 1550, archived: true }),
    rankingRow({ team: 'eq120', build: 'old', rating: 1400, archived: true }),
    rankingRow({ team: 'eq130', build: 'old', rating: 1500, archived: true }),
    rankingRow({ team: 'eq118', build: 'mid', rating: 1700, archived: true }),
  ];
  assert.deepEqual(withArchivedRows(active, old), [
    old[3],
    active[0],
    old[0],
    active[1],
    old[2],
    old[1],
  ]);
  assert.deepEqual(withArchivedRows(active, []), active);
  assert.equal(active.length, 2);
  assert.equal(must(old[0]).team, 'eq110');
});

void test('archived rows take their timings from the archived_paces of their team row', () => {
  const pace: TPace = { p50: 40, p95: 90, p99: 99, max: 120, count: 300, limit: 1000 };
  const active: TTeamRating[] = [
    teamRating({
      team: 'eq118',
      build: 'new',
      rating: 1600,
      pace: { ...pace, p95: 10 },
      archived_paces: { old: pace },
    }),
  ];
  const old: TRankingRow[] = [
    rankingRow({ team: 'eq118', build: 'old', rating: 1500, archived: true }),
    rankingRow({ team: 'eq118', build: 'older', rating: 1400, archived: true }),
  ];
  const [, withPace, without] = withArchivedRows(active, old);
  assert.deepEqual(withPace, { ...must(old[0]), pace });
  assert.equal(without, old[1]);
  assert.equal(must(old[0]).pace, undefined);
});

void test('past session rows stay visible and a selected matching archive is not duplicated', () => {
  const summaryVal = summary();
  const past: TTeamRating[] = [
    teamRating({ team: 'eq118', build: 'same' }),
    teamRating({ team: 'eq110', build: 'same' }),
  ];
  summaryVal.scopes['s1'] = scopeOf(past);
  const selected = archiveOptions(summaryVal).filter((opt) => opt.team === 'eq118');
  const old = archivedRows([game('1', 'same', 1520)], selected, { session: 's1' });
  assert.deepEqual(withArchivedRows(past, []), past);
  assert.deepEqual(withArchivedRows(past, old), past);
  // La même version reste ajoutable dans une session où l'équipe joue un autre build.
  const globalRatings = must(summaryVal.scopes['']).ratings;
  assert.deepEqual(withArchivedRows(globalRatings, old), [
    globalRatings[0],
    globalRatings[1],
    old[0],
  ]);
});

void test('last archived Elo follows timestamps when a tournament ID sorts after a newer ladder game', () => {
  const selected = archiveOptions(summary()).filter((opt) => opt.team === 'eq118');
  const recent = game('20261108T110000.000Z-eq118-bot', 'same', 1700, {
    ts: '2026-11-08T11:00:00.000Z',
  });
  const tournament = game('W3-1', 'same', 1900, {
    ts: '2026-11-08T10:00:00.000Z',
    tournament: { id: 'cup', match: 'W3', game: 1 },
  });
  const rows = Object.freeze([recent, tournament]);
  const archive = must(archivedRows(rows, selected, { session: 's1' })[0]);
  assert.equal(archive.rating, 1600);
  assert.equal(archive.last, recent.ts);
  assert.equal(archive.peak, 1800);
  assert.equal(archive.games, 2);
  assert.equal(must(archivedRows(rows, selected)[0]).games, 1);
});
