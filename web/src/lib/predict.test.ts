import { test } from 'node:test';
import assert from 'node:assert/strict';

import { percent, winProbability } from './predict.ts';

const model = {
  usable: false,
  red_advantage: 0.4,
  min_games: 10,
  teams: { a: { s: 1, n: 30 }, b: { s: 0, n: 30 }, new: { s: 0, n: 3 } },
};

test('une équipe plus forte est favorite, la couleur déplace la probabilité', () => {
  const red = winProbability(model, 'a', 'b', 'red');
  const black = winProbability(model, 'a', 'b', 'black');
  assert.ok(red > black && black > 0.5);
  assert.ok(Math.abs(red - 1 / (1 + Math.exp(-1.4))) < 1e-12);
  assert.ok(Math.abs(black - 1 / (1 + Math.exp(-0.6))) < 1e-12);
});

test("vue de l'autre côté, les deux probabilités se complètent", () => {
  const a = winProbability(model, 'a', 'b', 'red');
  const b = winProbability(model, 'b', 'a', 'black');
  assert.ok(Math.abs(a + b - 1) < 1e-12);
});

test('sans couleur : moyenne des deux couleurs', () => {
  const mean =
    (winProbability(model, 'a', 'b', 'red') + winProbability(model, 'a', 'b', 'black')) / 2;
  assert.equal(winProbability(model, 'a', 'b'), mean);
});

test('rien à afficher sans modèle, équipe inconnue ou trop peu de parties', () => {
  assert.equal(winProbability(null, 'a', 'b'), null);
  assert.equal(winProbability(model, 'a', 'inconnue'), null);
  assert.equal(winProbability(model, 'a', 'new'), null);
});

test('le pourcentage reste entre 1 et 99', () => {
  assert.equal(percent(0.634), 63);
  assert.equal(percent(0.9999), 99);
  assert.equal(percent(0.0001), 1);
});
