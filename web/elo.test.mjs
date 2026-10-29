import { test } from 'node:test'
import assert from 'node:assert/strict'

import { BETWEEN, gamesOf, isRemote, parseJsonl, ratingOf, since, START, versionStarts } from './elo.js'
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

test('gamesOf merges team files once each, in game order, with both modes', () => {
    const ab = game('a', 'b', 'red')
    const ac = game('a', 'c', 'black')
    const bc = game('b', 'c', 'red', { session: 's2' })
    const cd = game('c', 'd', 'red')
    const files = new Map([
        ['a', [ab, ac]],
        ['b', [ab, bc]],
        ['c', [ac, bc, cd]],
    ])
    const ids = (games) => games.map((g) => g.id)
    assert.deepEqual(ids(gamesOf(files, ['b', 'a'])), ids([ab, ac, bc]))
    assert.deepEqual(ids(gamesOf(files, ['a', 'b'], { mode: BETWEEN })), ids([ab]))
    assert.deepEqual(ids(gamesOf(files, ['a', 'b', 'c'], { mode: BETWEEN })), ids([ab, ac, bc]))
    assert.deepEqual(ids(gamesOf(files, ['b', 'c'], { mode: BETWEEN, session: 's2' })), ids([bc]))
    assert.deepEqual(ids(gamesOf(files, ['c'], { mode: BETWEEN })), ids([ac, bc, cd]))
    assert.deepEqual(gamesOf(files, []), [])
})

test('versionStarts marks each new build after the first entry', () => {
    const history = ['a', 'a', 'b', 'b', undefined, 'c'].map((build) => ({ build }))
    assert.deepEqual(versionStarts(history), [2, 5])
    assert.deepEqual(versionStarts([{ build: 'a' }]), [])
})

test('ratingOf reads verified ratings only when asked and present', () => {
    const side = { all: [1520, 20], session: [1510, 10], verified: { all: [1500, 0], session: [1490, -10] } }
    assert.deepEqual(ratingOf(side, 'all'), [1520, 20])
    assert.deepEqual(ratingOf(side, 'session', true), [1490, -10])
    assert.deepEqual(ratingOf({ all: [1520, 20] }, 'all', true), [1520, 20])
})

test('isRemote is true when either side played remotely', () => {
    assert.equal(isRemote(game('a', 'b', 'red')), false)
    assert.equal(isRemote(game('a', 'b', 'red', { black: { team: 'b', remote: true } })), true)
})
