import { test } from 'node:test';
import assert from 'node:assert/strict';

import { countdown, layout } from './queue.ts';

const teams = (n) => Array.from({ length: n }, (_, i) => ({ id: `t${i}`, ahead: i, bot: false }));
const marked = (nodes) => nodes.flatMap((n, i) => (n.path ? [i] : []));

test('15 équipes forment un arbre parfait, la médiane à la racine', () => {
  const nodes = layout(teams(15), 0);
  assert.deepEqual(
    nodes.map((n) => n.depth),
    [3, 2, 3, 1, 3, 2, 3, 0, 3, 2, 3, 1, 3, 2, 3],
  );
  assert.equal(nodes[7].parent, null);
  assert.deepEqual([nodes[3].parent, nodes[11].parent], [7, 7]);
});

test('le chemin descend de la racine à la prochaine équipe', () => {
  assert.deepEqual(marked(layout(teams(15), 0)), [0, 1, 3, 7]);
  assert.deepEqual(marked(layout(teams(15), 4)), [3, 4, 5, 7]);
});

test('chaque nœud garde son équipe et sa place dans la file', () => {
  const nodes = layout(teams(4), 0);
  assert.deepEqual(
    nodes.map((n) => n.id),
    ['t0', 't1', 't2', 't3'],
  );
  assert.deepEqual(
    nodes.map((n) => n.parent),
    [1, null, 1, 2],
  );
});

test('une file vide ou seule', () => {
  assert.deepEqual(layout([], 0), []);
  assert.deepEqual(layout(teams(1), 0), [
    { id: 't0', ahead: 0, bot: false, depth: 0, parent: null, path: true },
  ]);
});

test("le compte à rebours de l'annonce", () => {
  const at = '2026-11-05T12:01:00.000Z';
  const before = (s) => Date.parse(at) - s * 1000;
  assert.equal(countdown(at, before(42)), 'dans 42 s');
  assert.equal(countdown(at, before(41.2)), 'dans 42 s');
  assert.equal(countdown(at, before(65)), 'dans 1 min 05');
  assert.equal(countdown(at, before(0)), 'maintenant');
  assert.equal(countdown(at, before(-3)), 'maintenant');
});
