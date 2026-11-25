import { test } from 'node:test';
import assert from 'node:assert/strict';

import { eloGap, pairCounts, signTest, sprt, SPRT_EXPECTED, wilson } from './duel.ts';

// Valeurs attendues calculées par signTest, sprt, wilson et elo d'arena/web/app.js (arène isola).
const close = (actual: number, expected: number) =>
  assert.ok(Math.abs(actual - expected) < 1e-12, `${actual} ≠ ${expected}`);

test('test du signe identique à isola, même au-delà de 1075 paires', () => {
  assert.ok(Number.isNaN(signTest(0, 0)));
  for (const [a, b, p] of [
    [3, 0, 0.25000000000000006],
    [10, 3, 0.09228515625000003],
    [30, 20, 0.2026387510645394],
    [45, 15, 0.00013451408092849496],
    [20, 40, 0.013489293731191924],
    [60, 60, 1],
    [300, 280, 0.4301768979847971],
    [700, 650, 0.18231034946697916],
    [1100, 0, 0],
  ])
    close(signTest(a, b), p);
});

test('SPRT identique à isola', () => {
  const verdict = { 0: 'undecided', 1: 'a', 2: 'b', 3: 'close' } as const;
  for (const [a, b, v] of [
    [0, 0, 0],
    [3, 0, 0],
    [10, 3, 0],
    [30, 20, 0],
    [45, 15, 1],
    [20, 40, 0],
    [60, 60, 3],
    [300, 280, 3],
    [700, 650, 3],
    [1100, 0, 1],
    [19, 5, 0],
    [5, 19, 0],
    [40, 40, 3],
    [100, 100, 3],
    [18, 4, 0],
    [15, 45, 2],
  ] as const)
    assert.equal(sprt(a, b), verdict[v], `${a}-${b}`);
  assert.equal(SPRT_EXPECTED, 78);
});

test('Wilson et Elo identiques à isola', () => {
  for (const [k, n, low, high] of [
    [0, 10, -2.7755575615628914e-17, 0.2775401687666166],
    [7, 10, 0.39677321997956516, 0.892210712513788],
    [50, 100, 0.40382982859014716, 0.5961701714098528],
    [100, 100, 0.9630051925239981, 0.9999999999999999],
    [1, 3, 0.0614903152761605, 0.7923450448735121],
  ]) {
    const [l, h] = wilson(k, n);
    close(l, low);
    close(h, high);
  }
  assert.deepEqual(wilson(0, 0), [NaN, NaN]);
  for (const [s, e] of [
    [0.5, 0],
    [0.75, 190.848501887865],
    [0.25, -190.84850188786498],
    [0.9, 381.69700377572985],
  ])
    close(eloGap(s), e);
  assert.ok(Number.isNaN(eloGap(0)) && Number.isNaN(eloGap(1)));
});

test('paires 2-0 de chaque côté et paires 1-1', () => {
  const pair = (winners: [string, string]) => ({
    games: ['g1', 'g2'] as [string, string],
    winners,
    opening: [],
  });
  const pairs = [
    pair(['eq07', 'eq07']),
    pair(['eq12', 'eq12']),
    pair(['eq07', 'eq12']),
    pair(['eq12', 'eq07']),
    pair(['eq07', 'eq07']),
  ];
  assert.deepEqual(pairCounts(pairs, 'eq07'), { a: 2, b: 1, split: 2 });
  assert.deepEqual(pairCounts(pairs, 'eq12'), { a: 1, b: 2, split: 2 });
  assert.deepEqual(pairCounts([], 'eq07'), { a: 0, b: 0, split: 0 });
});
