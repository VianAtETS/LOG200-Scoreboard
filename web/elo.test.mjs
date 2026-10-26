import { test } from 'node:test'
import assert from 'node:assert/strict'

import {
    computeRatings,
    expected,
    moveTimes,
    parseJsonl,
    percentile,
    PROVISIONAL_GAMES,
    sessions,
    since,
    START,
    versions,
} from './elo.js'
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

test('expected score is symmetric', () => {
    assert.equal(expected(1500, 1500), 0.5)
    assert.ok(Math.abs(expected(1600, 1400) + expected(1400, 1600) - 1) < 1e-12)
})

test('first game moves both ratings by K/2', () => {
    const [a, b] = computeRatings([game('a', 'b', 'red')])
    assert.equal(a.team, 'a')
    assert.equal(a.rating, START + 20)
    assert.equal(b.rating, START - 20)
    assert.equal(a.wins, 1)
    assert.equal(b.losses, 1)
    assert.ok(a.provisional)
    assert.deepEqual(
        [a.history[0].delta, a.history[0].rating, b.history[0].delta],
        [20, START + 20, -20]
    )
    assert.equal(a.history[0].id, b.history[0].id)
})

test('anchor rating never moves', () => {
    const anchor = { team: 'ref', anchor: true }
    const g = game('a', 'x', 'black')
    g.black = anchor
    const ratings = computeRatings([g, game('ref', 'a', 'red')])
    const ref = ratings.find((r) => r.team === 'ref')
    assert.equal(ref.rating, START)
    assert.equal(ref.provisional, false)
    assert.equal(ref.games, 2)
    assert.ok(ref.history.every((h) => h.delta === 0))
})

test('K drops after the provisional period and resets on a new build', () => {
    const games = []
    for (let i = 0; i < PROVISIONAL_GAMES; i++) {
        const g = game('a', 'b', i % 2 ? 'red' : 'black')
        g.red.build = 'v1'
        games.push(g)
    }
    let a = computeRatings(games).find((r) => r.team === 'a')
    assert.equal(a.provisional, false)
    const next = game('a', 'b', 'red')
    next.red.build = 'v2'
    a = computeRatings([...games, next]).find((r) => r.team === 'a')
    assert.equal(a.provisional, true)
    assert.equal(a.gamesOnBuild, 1)
    assert.equal(a.games, PROVISIONAL_GAMES + 1)
})

test('games are processed in timestamp order and filtered by session', () => {
    const g1 = game('a', 'b', 'red')
    const g2 = game('a', 'b', 'black', { session: 's2' })
    const reversed = computeRatings([g2, g1])
    const ordered = computeRatings([g1, g2])
    assert.deepEqual(reversed, ordered)
    assert.equal(computeRatings([g1, g2], 's2')[0].games, 1)
    assert.deepEqual(sessions([g1, g2]), ['s2', 's1'])
})

test('versions number builds per team across sessions, reverting keeps the old number', () => {
    const builds = ['h1', 'h2', 'h1', 'h3']
    const list = builds.map((b, i) => {
        const g = game('a', 'b', 'red', { session: `s${i}` })
        g.red.build = b
        return g
    })
    assert.deepEqual(versions(list.reverse()).get('a'), ['h1', 'h2', 'h3'])
    assert.equal(versions(list).has('b'), false)
})

test('moveTimes splits by color and ignores missing or inconsistent data', () => {
    const g = game('a', 'b', 'red', { moves: ['x', 'y', 'z'], move_ms: [10, 20, 30] })
    assert.deepEqual(moveTimes(g, 'red'), [10, 30])
    assert.deepEqual(moveTimes(g, 'black'), [20])
    assert.deepEqual(moveTimes({ ...g, move_ms: [10] }, 'red'), [])
    assert.deepEqual(moveTimes(game('a', 'b', 'red'), 'red'), [])
})

test('percentile uses nearest rank', () => {
    const sorted = Array.from({ length: 100 }, (_, i) => i + 1)
    assert.equal(percentile(sorted, 0.5), 50)
    assert.equal(percentile(sorted, 0.95), 95)
    assert.equal(percentile(sorted, 0.99), 99)
    assert.equal(percentile([7], 0.99), 7)
})

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
    const [r] = computeRatings([game('a', 'z', 'red'), game('a', 'z', 'red'), game('a', 'z', 'red')]).filter(
        (r) => r.team === 'a'
    )
    const at = (i) => Date.parse(r.history[i].ts)
    assert.deepEqual(since(r.history, -Infinity), { start: START, points: r.history })
    assert.deepEqual(since(r.history, at(1)), { start: r.history[0].rating, points: r.history.slice(1) })
    assert.deepEqual(since(r.history, at(2) + 1), { start: r.history[2].rating, points: [] })
})
