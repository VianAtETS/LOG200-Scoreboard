import { test } from 'node:test'
import assert from 'node:assert/strict'

import { parseJsonl, since, START, versionStarts } from './elo.js'
import { BLACK, DESTROYED, EMPTY, positions, RED, squareIndex, squareName } from './replay.js'

let clock = 0
function game(red, black, winner, extra = {}) {
    clock++
    return {
        v: 1,
        id: `g${clock}`,
        session: 's1',
        ts: `2026-10-24T12:00:${String(clock).padStart(2, '0')}.000Z`,
        red: { team: red },
        black: { team: black },
        winner,
        moves: [],
        ...extra,
    }
}

test('parseJsonl skips truncated lines and unknown versions', () => {
    const line = JSON.stringify(game('a', 'b', 'red'))
    const { games, skipped } = parseJsonl(`${line}\n{"v":2}\n${line.slice(0, 20)}`)
    assert.equal(games.length, 1)
    assert.equal(skipped, 2)
})

test('replay squares follow server indices', () => {
    assert.equal(squareIndex('G', 1), 138)
    assert.equal(squareIndex('F', 12), 5)
    assert.equal(squareName(138), 'G1')
    const steps = positions({ opening: ['G1-G4-D7'], moves: ['F12-F9-G2'] })
    assert.equal(steps.length, 3)
    const last = steps[2].cells
    assert.equal(last[squareIndex('G', 4)], RED)
    assert.equal(last[squareIndex('F', 9)], BLACK)
    assert.equal(last[squareIndex('G', 1)], EMPTY)
    assert.equal(last[squareIndex('D', 7)], DESTROYED)
    assert.equal(last[squareIndex('G', 2)], DESTROYED)
    assert.ok(steps[1].opening && !steps[2].opening)
})

test('since starts from the rating just before the cutoff', () => {
    const history = [1, 2, 3].map((i) => ({ ts: `2026-10-24T12:00:0${i}.000Z`, rating: START + 10 * i }))
    const at = (i) => Date.parse(history[i].ts)
    assert.deepEqual(since(history, -Infinity), { start: START, points: history })
    assert.deepEqual(since(history, at(1)), { start: history[0].rating, points: history.slice(1) })
    assert.deepEqual(since(history, at(2) + 1), { start: history[2].rating, points: [] })
})

test('versionStarts marks each new build after the first entry', () => {
    const history = ['a', 'a', 'b', 'b', undefined, 'c'].map((build) => ({ build }))
    assert.deepEqual(versionStarts(history), [2, 5])
    assert.deepEqual(versionStarts([{ build: 'a' }]), [])
})
