import assert from 'node:assert/strict';
import { test } from 'node:test';
import { countdown, layout } from '@/lib/queue';
import type { TQueueNode } from '@/lib/queue';
import type { TQueueTeam } from '@/types/summary.types';

function must<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) {
    throw new Error('expected defined value');
  }
  return value;
}

function teams(count: number): TQueueTeam[] {
  return Array.from({ length: count }, (_, i) => ({ id: `t${i}`, ahead: i, bot: false }));
}

function marked(nodes: readonly TQueueNode[]): number[] {
  return nodes.flatMap((node, i) => (node.path ? [i] : []));
}

void test('15 équipes forment un arbre parfait, la médiane à la racine', () => {
  const nodes = layout(teams(15), 0);
  assert.deepEqual(
    nodes.map((node) => node.depth),
    [3, 2, 3, 1, 3, 2, 3, 0, 3, 2, 3, 1, 3, 2, 3],
  );
  assert.equal(must(nodes[7]).parent, null);
  assert.deepEqual([must(nodes[3]).parent, must(nodes[11]).parent], [7, 7]);
});

void test('le chemin descend de la racine à la prochaine équipe', () => {
  const queue = teams(15);
  assert.deepEqual(marked(layout(queue, 0)), [0, 1, 3, 7]);
  assert.deepEqual(marked(layout(queue, 4)), [3, 4, 5, 7]);
});

void test('chaque nœud garde son équipe et sa place dans la file', () => {
  const nodes = layout(teams(4), 0);
  assert.deepEqual(
    nodes.map((node) => node.id),
    ['t0', 't1', 't2', 't3'],
  );
  assert.deepEqual(
    nodes.map((node) => node.parent),
    [1, null, 1, 2],
  );
});

void test('une file vide ou seule', () => {
  assert.deepEqual(layout([], 0), []);
  assert.deepEqual(layout(teams(1), 0), [
    { id: 't0', ahead: 0, bot: false, depth: 0, parent: null, path: true },
  ]);
});

void test("le compte à rebours de l'annonce", () => {
  const at = '2026-11-05T12:01:00.000Z';
  const before = (seconds: number): number => Date.parse(at) - seconds * 1000;
  assert.equal(countdown(at, before(42)), 'dans 42 s');
  assert.equal(countdown(at, before(41.2)), 'dans 42 s');
  assert.equal(countdown(at, before(65)), 'dans 1 min 05');
  assert.equal(countdown(at, before(0)), 'maintenant');
  assert.equal(countdown(at, before(-3)), 'maintenant');
});
