import assert from 'node:assert/strict';
import test from 'node:test';

import { apply, ENDED_KEEP_MS, forget, liveUrl, mover, pickLive } from './live.ts';

const game = (id, extra = {}) => ({
  id,
  red: 'a',
  black: 'b',
  opening: [],
  moves: [],
  move_ms: [],
  turn_ms: 0,
  ...extra,
});
const snapshot = (...games) => ({ v: 1, type: 'snapshot', games });

test("l'adresse suit le serveur et passe en ws ou wss", () => {
  assert.equal(liveUrl('..', 'http://ladder.local/web/index.html'), 'ws://ladder.local/api/live');
  assert.equal(
    liveUrl('https://ladder.example.com', 'https://user.github.io/x/'),
    'wss://ladder.example.com/api/live',
  );
});

test('les coups suivent le snapshot, un trou demande une resynchronisation', () => {
  let games = apply(
    new Map(),
    snapshot(game('g1', { moves: ['G1-G4-F9'], move_ms: [5], turn_ms: 300 })),
    1000,
  );
  assert.equal(games.get('g1').turnAt, 700);
  games = apply(games, { v: 1, type: 'move', id: 'g1', n: 1, move: 'F12-F10-G7', ms: 9 }, 2000);
  assert.deepEqual(games.get('g1').moves, ['G1-G4-F9', 'F12-F10-G7']);
  assert.equal(games.get('g1').turnAt, 2000);
  assert.equal(apply(games, { v: 1, type: 'move', id: 'g1', n: 5, move: 'x', ms: 1 }, 0), 'resync');
});

test('une fin garde la partie, annulée ou non', () => {
  let games = apply(new Map(), { v: 1, type: 'started', game: game('g1') }, 0);
  games = apply(games, { v: 1, type: 'ended', id: 'g1', winner: 'red', reason: 'no_moves' }, 50);
  assert.deepEqual(games.get('g1').ended, { winner: 'red', reason: 'no_moves', cancelled: false });
  games = apply(games, { v: 1, type: 'started', game: game('g2') }, 0);
  games = apply(games, { v: 1, type: 'ended', id: 'g2', cancelled: true }, 60);
  assert.equal(games.get('g2').ended.cancelled, true);
  assert.equal(
    apply(games, { v: 1, type: 'move', id: 'g1', n: 0, move: 'x', ms: 1 }, 0),
    games,
    'coup après la fin ignoré',
  );
});

test("une partie absente du nouveau snapshot est interrompue, l'inconnu est ignoré", () => {
  let games = apply(new Map(), snapshot(game('g1'), game('g2')), 0);
  games = apply(games, snapshot(game('g2')), 10);
  assert.deepEqual(games.get('g1').ended, { interrupted: true });
  assert.equal(apply(games, { v: 1, type: 'futur', extra: 1 }, 0), games);
  assert.equal(apply(games, { v: 2, type: 'snapshot', games: [] }, 0), 'outdated');
});

test('le trait alterne, ouverture comprise', () => {
  assert.equal(mover(game('g', { opening: ['a', 'b'] })), 'red');
  assert.equal(mover(game('g', { opening: ['a', 'b'], moves: ['c'] })), 'black');
});

test('une partie finie est oubliée après le délai', () => {
  let games = apply(new Map(), { v: 1, type: 'started', game: game('g1') }, 0);
  games = apply(games, { v: 1, type: 'ended', id: 'g1', winner: 'red', reason: 'timeout' }, 100);
  assert.equal(forget(games, 100 + ENDED_KEEP_MS - 1), games);
  assert.equal(forget(games, 100 + ENDED_KEEP_MS).size, 0);
});

test('la partie suivie est la plus récente des équipes choisies, selon le filtre', () => {
  const games = new Map([
    ['g1', game('g1', { red: 'a', black: 'b', since: '2026-11-01T10:00:00Z' })],
    ['g2', game('g2', { red: 'c', black: 'a', since: '2026-11-01T10:05:00Z' })],
    [
      'g3',
      game('g3', { red: 'a', black: 'd', since: '2026-11-01T10:09:00Z', ended: { winner: 'red' } }),
    ],
  ]);
  assert.equal(pickLive(games, ['a'], false).id, 'g2', 'la plus récente non finie');
  assert.equal(
    pickLive(games, ['a'], true).id,
    'g2',
    'entre elles avec une seule équipe garde ses parties',
  );
  assert.equal(
    pickLive(games, ['a', 'b'], true).id,
    'g1',
    'entre elles : les deux équipes choisies',
  );
  assert.equal(pickLive(games, ['d'], false), undefined, 'partie finie exclue');
  assert.equal(pickLive(games, [], false), undefined);
});

test('the followed game receives its end state when no other game is active', () => {
  let games = apply(new Map(), snapshot(game('g1', { since: '2026-11-01T10:00:00Z' })), 0);
  const followed = pickLive(games, ['a'], true).id;
  games = apply(
    games,
    { v: 1, type: 'ended', id: followed, winner: 'red', reason: 'no_moves' },
    50,
  );

  assert.deepEqual(pickLive(games, ['a'], true, followed)?.ended, {
    winner: 'red',
    reason: 'no_moves',
    cancelled: false,
  });
});

test('the live fallback prioritizes active games and respects the filter', () => {
  const games = new Map([
    ['g1', game('g1', { since: '2026-11-01T10:09:00Z', ended: { winner: 'red' } })],
    ['g2', game('g2', { red: 'a', black: 'c', since: '2026-11-01T10:05:00Z' })],
  ]);

  assert.equal(pickLive(games, ['a'], false, 'g1').id, 'g2');
  assert.equal(pickLive(games, ['unknown'], false, 'g1'), undefined);
  assert.equal(pickLive(games, ['b', 'c'], true, 'g1'), undefined);
  assert.equal(pickLive(games, [], false, 'g1'), undefined);
});
