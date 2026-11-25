import assert from 'node:assert/strict';
import { test } from 'node:test';
import { valueAt } from '@/components/bump/bump.utils';

type TVertex = Parameters<typeof valueAt>[0][number];
const VERTEX_INDEX_PATTERN = /^(?:0|[1-9]\d*)$/u;

function isStringKey(prop: string | symbol): prop is string {
  return Object.prototype.toString.call(prop) === '[object String]';
}

void test('valueAt uses at most 100 vertex reads to interpolate in a sorted 65k-vertex path', () => {
  const path: TVertex[] = Array.from({ length: 65_536 }, (_, x): TVertex => [x, x * 2 + 5]);
  let reads = 0;
  const observed = new Proxy(path, {
    get(target, property): number | TVertex | undefined {
      if (property === 'length') {
        return target.length;
      }
      if (isStringKey(property) && VERTEX_INDEX_PATTERN.test(property)) {
        reads += 1;
        return target[Number(property)];
      }
      return undefined;
    },
  });
  assert.equal(valueAt(observed, 60_000.5), 120_006);
  assert.ok(reads <= 100, `expected at most 100 numeric vertex reads, got ${reads}`);
});
