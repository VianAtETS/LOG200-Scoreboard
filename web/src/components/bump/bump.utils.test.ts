import { test } from 'node:test';
import assert from 'node:assert/strict';

import { valueAt } from './bump.utils.ts';

test('valueAt uses at most 100 vertex reads to interpolate in a sorted 65k-vertex path', () => {
  const path = Array.from({ length: 65_536 }, (_, x): [number, number] => [x, x * 2 + 5]);
  let reads = 0;
  const observed = new Proxy(path, {
    get(target, property, receiver) {
      if (typeof property === 'string' && /^(0|[1-9]\d*)$/.test(property)) reads++;
      return Reflect.get(target, property, receiver);
    },
  });

  assert.equal(valueAt(observed, 60_000.5), 120_006);
  assert.ok(reads <= 100, `expected at most 100 numeric vertex reads, got ${reads}`);
});
