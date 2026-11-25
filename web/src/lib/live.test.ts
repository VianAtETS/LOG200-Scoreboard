import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ENDED_KEEP_MS, apply, forget, liveUrl, mover, pickLive } from '@/lib/live';
import type { TLiveGame, TLiveGames, TLiveMessage } from '@/types/live.types';

function must<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) {
    throw new Error('expected defined value');
  }
  return value;
}

function liveMap(res: TLiveGames | 'outdated' | 'resync'): TLiveGames {
  if (res === 'outdated' || res === 'resync') {
    throw new Error(`expected live games map, got ${res}`);
  }
  return res;
}

function game(id: string, extra: Partial<TLiveGame> = {}): TLiveGame {
  return {
    id,
    red: 'a',
    black: 'b',
    since: '2026-11-01T10:00:00Z',
    time_ms: 1000,
    opening: [],
    moves: [],
    move_ms: [],
    turn_ms: 0,
    turnAt: 0,
    ...extra,
  };
}

function snapshot(...games: TLiveGame[]): TLiveMessage {
  return { v: 1, type: 'snapshot', games };
}

void test("l'adresse suit le serveur et passe en ws ou wss", () => {
  assert.equal(liveUrl('..', 'http://ladder.local/web/index.html'), 'ws://ladder.local/api/live');
  assert.equal(
    liveUrl('https://ladder.example.com', 'https://user.github.io/x/'),
    'wss://ladder.example.com/api/live',
  );
});

void test('les coups suivent le snapshot, un trou demande une resynchronisation', () => {
  const initial = snapshot(game('g1', { moves: ['G1-G4-F9'], move_ms: [5], turn_ms: 300 }));
  let games = liveMap(apply(new Map(), initial, 1000));
  assert.equal(must(games.get('g1')).turnAt, 700);
  games = liveMap(
    apply(games, { v: 1, type: 'move', id: 'g1', n: 1, move: 'F12-F10-G7', ms: 9 }, 2000),
  );
  assert.deepEqual(must(games.get('g1')).moves, ['G1-G4-F9', 'F12-F10-G7']);
  assert.equal(must(games.get('g1')).turnAt, 2000);
  assert.equal(apply(games, { v: 1, type: 'move', id: 'g1', n: 5, move: 'x', ms: 1 }, 0), 'resync');
});

void test('une fin garde la partie, annulée ou non', () => {
  let games = liveMap(apply(new Map(), { v: 1, type: 'started', game: game('g1') }, 0));
  games = liveMap(
    apply(games, { v: 1, type: 'ended', id: 'g1', winner: 'red', reason: 'no_moves' }, 50),
  );
  assert.deepEqual(must(games.get('g1')).ended, {
    winner: 'red',
    reason: 'no_moves',
    cancelled: false,
  });
  games = liveMap(apply(games, { v: 1, type: 'started', game: game('g2') }, 0));
  games = liveMap(apply(games, { v: 1, type: 'ended', id: 'g2', cancelled: true }, 60));
  const endedGame = must(games.get('g2'));
  assert.equal(must(endedGame.ended).cancelled, true);
  assert.equal(
    apply(games, { v: 1, type: 'move', id: 'g1', n: 0, move: 'x', ms: 1 }, 0),
    games,
    'coup après la fin ignoré',
  );
});

void test("une partie absente du nouveau snapshot est interrompue, l'inconnu est ignoré", () => {
  const snapBoth = snapshot(game('g1'), game('g2'));
  const snapSecond = snapshot(game('g2'));
  let games = liveMap(apply(new Map(), snapBoth, 0));
  games = liveMap(apply(games, snapSecond, 10));
  assert.deepEqual(must(games.get('g1')).ended, { interrupted: true });
  const futureMsg = { v: 1, type: 'futur', extra: 1 };
  // @ts-expect-error: An event from a newer server can have a type this client does not know.
  assert.equal(apply(games, futureMsg, 0), games);
  assert.equal(apply(games, { v: 2, type: 'snapshot', games: [] }, 0), 'outdated');
});

void test('le trait alterne, ouverture comprise', () => {
  assert.equal(mover(game('g', { opening: ['a', 'b'] })), 'red');
  assert.equal(mover(game('g', { opening: ['a', 'b'], moves: ['c'] })), 'black');
});

void test('une partie finie est oubliée après le délai', () => {
  let games = liveMap(apply(new Map(), { v: 1, type: 'started', game: game('g1') }, 0));
  games = liveMap(
    apply(games, { v: 1, type: 'ended', id: 'g1', winner: 'red', reason: 'timeout' }, 100),
  );
  assert.equal(forget(games, 100 + ENDED_KEEP_MS - 1), games);
  assert.equal(forget(games, 100 + ENDED_KEEP_MS).size, 0);
});

void test('la partie suivie est la plus récente des équipes choisies, selon le filtre', () => {
  const games: TLiveGames = new Map([
    ['g1', game('g1', { red: 'a', black: 'b', since: '2026-11-01T10:00:00Z' })],
    ['g2', game('g2', { red: 'c', black: 'a', since: '2026-11-01T10:05:00Z' })],
    [
      'g3',
      game('g3', {
        red: 'a',
        black: 'd',
        since: '2026-11-01T10:09:00Z',
        ended: { winner: 'red' },
      }),
    ],
  ]);
  assert.equal(must(pickLive(games, ['a'], false)).id, 'g2', 'la plus récente non finie');
  assert.equal(
    must(pickLive(games, ['a'], true)).id,
    'g2',
    'entre elles avec une seule équipe garde ses parties',
  );
  assert.equal(
    must(pickLive(games, ['a', 'b'], true)).id,
    'g1',
    'entre elles : les deux équipes choisies',
  );
  assert.equal(pickLive(games, ['d'], false), undefined, 'partie finie exclue');
  assert.equal(pickLive(games, [], false), undefined);
});

void test('the followed game receives its end state when no other game is active', () => {
  const initial = snapshot(game('g1', { since: '2026-11-01T10:00:00Z' }));
  let games = liveMap(apply(new Map(), initial, 0));
  const followed = must(pickLive(games, ['a'], true)).id;
  games = liveMap(
    apply(games, { v: 1, type: 'ended', id: followed, winner: 'red', reason: 'no_moves' }, 50),
  );
  assert.deepEqual(pickLive(games, ['a'], true, followed)?.ended, {
    winner: 'red',
    reason: 'no_moves',
    cancelled: false,
  });
});

void test('the live fallback prioritizes active games and respects the filter', () => {
  const games: TLiveGames = new Map([
    ['g1', game('g1', { since: '2026-11-01T10:09:00Z', ended: { winner: 'red' } })],
    ['g2', game('g2', { red: 'a', black: 'c', since: '2026-11-01T10:05:00Z' })],
  ]);
  assert.equal(must(pickLive(games, ['a'], false, 'g1')).id, 'g2');
  assert.equal(pickLive(games, ['unknown'], false, 'g1'), undefined);
  assert.equal(pickLive(games, ['b', 'c'], true, 'g1'), undefined);
  assert.equal(pickLive(games, [], false, 'g1'), undefined);
});
