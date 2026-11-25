import assert from 'node:assert/strict';
import { test } from 'node:test';

import { percent, winProbability } from '@/lib/predict';

function must<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) {
    throw new Error('expected defined value');
  }
  return value;
}

const model = {
  usable: false,
  red_advantage: 0.4,
  min_games: 10,
  teams: { a: { s: 1, n: 30 }, b: { s: 0, n: 30 }, new: { s: 0, n: 3 } },
};

void test('une équipe plus forte est favorite, la couleur déplace la probabilité', () => {
  const red = must(winProbability(model, 'a', 'b', 'red'));
  const black = must(winProbability(model, 'a', 'b', 'black'));
  assert.ok(red > black && black > 0.5);
  assert.ok(Math.abs(red - 1 / (1 + Math.exp(-1.4))) < 1e-12);
  assert.ok(Math.abs(black - 1 / (1 + Math.exp(-0.6))) < 1e-12);
});

void test("vue de l'autre côté, les deux probabilités se complètent", () => {
  const probA = must(winProbability(model, 'a', 'b', 'red'));
  const probB = must(winProbability(model, 'b', 'a', 'black'));
  assert.ok(Math.abs(probA + probB - 1) < 1e-12);
});

void test('sans couleur : moyenne des deux couleurs', () => {
  const mean =
    (must(winProbability(model, 'a', 'b', 'red')) +
      must(winProbability(model, 'a', 'b', 'black'))) /
    2;
  assert.equal(winProbability(model, 'a', 'b'), mean);
});

void test('rien à afficher sans modèle, équipe inconnue ou trop peu de parties', () => {
  assert.equal(winProbability(null, 'a', 'b'), null);
  assert.equal(winProbability(model, 'a', 'inconnue'), null);
  assert.equal(winProbability(model, 'a', 'new'), null);
});

void test('le pourcentage reste entre 1 et 99', () => {
  assert.equal(percent(0.634), 63);
  assert.equal(percent(0.9999), 99);
  assert.equal(percent(0.0001), 1);
});
