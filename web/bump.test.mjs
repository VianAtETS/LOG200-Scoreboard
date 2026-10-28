import { test } from 'node:test'
import assert from 'node:assert/strict'

import { pointAt, rankSeries, smooth, topTeams } from './bump.js'

let clock = 0
// Ligne de timeline : `red` et `black` sont [équipe, rating après la partie, build].
function tick([red, rr, rb], [black, br, bb], session = 's1') {
    clock++
    const side = (team, rating, build) => ({
        team,
        ...(build ? { build } : {}),
        all: [rating, 0],
        session: [rating - 100, 0],
    })
    return {
        v: 1,
        id: `g${clock}`,
        ts: `2026-10-24T12:${String(clock).padStart(2, '0')}:00.000Z`,
        session,
        red: side(red, rr, rb),
        black: side(black, br, bb),
    }
}

test('top teams skip bots', () => {
    const ratings = [{ team: 'a', build: 'x' }, { team: 'bot' }, { team: 'b', build: 'y' }, { team: 'c', build: 'z' }]
    assert.deepEqual(topTeams(ratings, 2), ['a', 'b'])
})

test('ranks follow every game and branches start at the first game', () => {
    const rows = [
        tick(['a', 1520, 'a1'], ['b', 1480, 'b1']),
        tick(['c', 1540, 'c1'], ['b', 1460, 'b1']),
        tick(['b', 1500, 'b1'], ['a', 1500, 'a1']),
    ]
    const { columns, series } = rankSeries(rows, { selected: ['a', 'b', 'c'] })
    assert.equal(columns.length, 3)
    assert.deepEqual(series.get('a').map((p) => [p.x, p.rank]), [[0, 1], [1, 2], [2, 2]])
    // Égalité à 1500 : départagée par le nom, a avant b.
    assert.deepEqual(series.get('b').map((p) => [p.x, p.rank]), [[0, 2], [1, 3], [2, 3]])
    // c n'existe qu'à partir de sa première partie.
    assert.deepEqual(series.get('c').map((p) => [p.x, p.rank]), [[1, 1]])
    assert.equal(pointAt(series.get('c'), 0), undefined)
    assert.equal(pointAt(series.get('c'), 2).rank, 1)
})

test('unchanged ranks add no point', () => {
    const rows = [
        tick(['a', 1520, 'a1'], ['b', 1480, 'b1']),
        tick(['c', 1510, 'c1'], ['d', 1490, 'd1']),
    ]
    const { series } = rankSeries(rows, { selected: ['a'] })
    assert.deepEqual(series.get('a').map((p) => p.x), [0])
})

test('bots take no rank and sit between two teams', () => {
    const rows = [
        tick(['a', 1520, 'a1'], ['bot', 1500]),
        tick(['b', 1480, 'b1'], ['bot', 1500]),
    ]
    const { series } = rankSeries(rows, { selected: ['a', 'b', 'bot'] })
    assert.equal(series.get('b').at(-1).rank, 2)
    assert.equal(series.get('bot').at(-1).rank, 1.5)
    assert.equal(series.get('bot').at(-1).bot, true)
})

test('a new build is a version point, even without a rank change', () => {
    const rows = [
        tick(['a', 1520, 'a1'], ['b', 1480, 'b1']),
        tick(['a', 1520, 'a2'], ['c', 1400, 'c1']),
        tick(['a', 1520, 'a2'], ['c', 1400, 'c1']),
    ]
    const { series } = rankSeries(rows, { selected: ['a'] })
    assert.deepEqual(series.get('a').map((p) => [p.x, p.version, p.build]), [[0, false, 'a1'], [1, true, 'a2']])
})

test('session uses session ratings and only its games', () => {
    const rows = [
        tick(['a', 1700, 'a1'], ['b', 1300, 'b1'], 's1'),
        tick(['b', 1620, 'b1'], ['a', 1580, 'a1'], 's2'),
    ]
    const { columns, series } = rankSeries(rows, { key: 'session', session: 's2', selected: ['a', 'b'] })
    assert.deepEqual(columns.map((g) => g.id), [rows[1].id])
    assert.deepEqual(series.get('b'), [{ x: 0, rank: 1, rating: 1520, build: 'b1', version: false, bot: false }])
})

test('the window starts at the cutoff with the ratings reached before it', () => {
    const rows = [
        tick(['a', 1520, 'a1'], ['b', 1480, 'b1']),
        tick(['a', 1540, 'a1'], ['c', 1480, 'c1']),
        tick(['b', 1500, 'b1'], ['c', 1460, 'c1']),
    ]
    const { columns, series } = rankSeries(rows, { selected: ['b', 'c'], cutoff: Date.parse(rows[2].ts) })
    assert.equal(columns.length, 1)
    assert.deepEqual(series.get('b').map((p) => [p.x, p.rank, p.rating]), [[0, 2, 1500]])
    assert.deepEqual(series.get('c').map((p) => [p.x, p.rank]), [[0, 3]])
})

test('smooth averages the rank over each bucket, from the start of the branch', () => {
    // Rang 2 de 5 à 9, rang 1 de 10 à 14, puis 2 à partir de 15 ; 20 colonnes en 4 paquets.
    const points = [
        { x: 5, rank: 2 },
        { x: 10, rank: 1 },
        { x: 15, rank: 2 },
    ]
    assert.deepEqual(smooth(points, 20, 4), [
        { x: 7, rank: 2 },
        { x: 12, rank: 1 },
        { x: 17, rank: 2 },
    ])
    // Échanges de place à chaque partie : la moyenne se tient entre les deux rangs.
    const flips = Array.from({ length: 12 }, (_, x) => ({ x, rank: (x % 2) + 1 }))
    assert.deepEqual(smooth(flips, 12, 3).map((p) => p.rank), [1.5, 1.5, 1.5])
    assert.equal(smooth(points, 20, 40), points)
})
