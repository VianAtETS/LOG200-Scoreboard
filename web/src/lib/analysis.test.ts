import { test } from 'node:test'
import assert from 'node:assert/strict'

import { analyse, describe } from './analysis.ts'
import { positions } from './replay.ts'

// Première partie de crates/isolation-ladder/tests/fixtures/golden/games.jsonl. Valeurs attendues calculées
// par measure() de l'arène isola (arena/web/core.js), l'implémentation de référence.
const game = {
    winner: 'red' as const,
    reason: 'no_moves',
    opening: ['G1-I1-C12', 'F12-D10-F1'],
    moves: ['I1-F4-A7', 'D10-C10-I7', 'F4-F6-F10', 'C10-A10-F3', 'F6-H4-D10', 'A10-B11-K7', 'H4-E7-E11', 'B11-A11-B10',
        'E7-E6-C11', 'A11-B12-C6', 'E6-G8-D12', 'B12-A11-I10', 'G8-G6-A9', 'A11-A10-I8', 'G6-F6-C8', 'A10-A12-H6',
        'F6-E5-C10', 'A12-A10-E3', 'E5-E7-A11', 'A10-B11-G5', 'E7-D6-A10', 'B11-B12-B6', 'D6-E7-B11', 'B12-A12-F7',
        'E7-D8-B12'],
}

const a = analyse(positions(game), game)

test('mobilité, territoire et région identiques à l’arène isola après chaque coup', () => {
    assert.deepEqual(a.red.mob, [15, 15, 14, 23, 22, 24, 23, 24, 23, 24, 23, 24, 22, 24, 22, 24, 22, 22, 20, 24, 22, 23, 21, 20, 20, 21, 18, 12])
    assert.deepEqual(a.black.mob, [15, 14, 21, 20, 18, 17, 11, 10, 13, 12, 8, 6, 3, 3, 6, 4, 6, 4, 6, 4, 4, 2, 3, 2, 2, 1, 1, 0])
    assert.deepEqual(a.red.terr, [63, 58, 42, 63, 69, 80, 98, 78, 76, 95, 110, 115, 121, 120, 109, 116, 91, 106, 105, 118, 109, 113, 116, 117, 116, 116, 115, 115])
    assert.deepEqual(a.black.terr, [63, 62, 67, 35, 28, 20, 15, 24, 27, 11, 10, 6, 3, 5, 6, 4, 10, 7, 10, 4, 5, 4, 3, 2, 2, 1, 1, 0])
    assert.deepEqual(a.red.reg.slice(19), [123, 118, 118, 118, 117, 116, 116, 115, 115])
    assert.deepEqual(a.black.reg.slice(19), [123, 122, 121, 120, 2, 2, 1, 1, 0])
})

test('coups critiques : les 8 plus grands écarts de territoire des équipes, hors ouverture', () => {
    assert.deepEqual([...a.crit].sort(([x], [y]) => x - y), [
        [3, 'fine'], [5, 'fine'], [6, 'bad'], [7, 'bad'], [9, 'fine'], [16, 'fine'], [17, 'fine'], [19, 'fine'],
    ])
})

test('point de bascule : après lui, le perdant ne reprend jamais l’avantage en territoire', () => {
    assert.equal(a.tip, 3)
    assert.equal(analyse(positions(game), { ...game, reason: 'timeout' }).tip, null)
})

test('describe donne min, moyenne, médiane, max, écart type et effectif', () => {
    assert.deepEqual(describe([4, 1, 3, 2]), { min: 1, mean: 2.5, median: 2.5, max: 4, sd: Math.sqrt(1.25), n: 4 })
    assert.equal(describe([]), null)
})
