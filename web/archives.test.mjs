import { test } from 'node:test'
import assert from 'node:assert/strict'
import { archiveKey, archiveOptions, archivedRows, currentBuilds } from './archives.js'

const summary = () => ({
    builds: { eq110: ['same', 'new110'], eq118: ['same', 'new118'] },
    scopes: { '': { ratings: [{ team: 'eq110', build: 'new110', last: '2' }, { team: 'eq118', build: 'new118', last: '2' }] } },
})
const game = (id, build, rating, extra = {}) => ({ id, ts: id, session: 's1', winner: 'red',
    red: { team: 'eq118', build, all: [rating, 20], session: [rating - 100, 10] },
    black: { team: 'bot', all: [1500, 0], session: [1500, 0] }, ...extra })

test('archives are identified by team and build, with stable global vN labels', () => {
    const options = archiveOptions(summary())
    assert.deepEqual(options.map((o) => [o.team, o.version, o.build]), [['eq110', 1, 'same'], ['eq118', 1, 'same']])
    assert.notEqual(options[0].key, options[1].key)
    assert.equal(archiveKey('team,unsafe', 'x'), 'team%2Cunsafe@x')
})

test('a restored build becomes current again without renumbering old versions', () => {
    const s = summary()
    s.scopes.s2 = { ratings: [{ team: 'eq118', build: 'same', last: '3' }] }
    assert.equal(currentBuilds(s).get('eq118'), 'same')
    assert.deepEqual(archiveOptions(s).filter((o) => o.team === 'eq118').map((o) => [o.version, o.build]), [[2, 'new118']])
})

test('archived statistics stop at that hash and do not mutate the active ranking', () => {
    const s = summary(), before = JSON.stringify(s)
    const selected = archiveOptions(s).filter((o) => o.team === 'eq118')
    const result = archivedRows([game('1', 'same', 1520), game('2', 'same', 1500, { winner: 'black' }),
        game('3', 'new118', 1700)], selected)
    assert.equal(result.length, 1)
    assert.deepEqual([result[0].rating, result[0].peak, result[0].games, result[0].wins], [1500, 1520, 2, 1])
    assert.equal(result[0].archived, true)
    assert.equal(JSON.stringify(s), before)
    assert.deepEqual(archivedRows([game('1', 'same', 1520)], []), [])
})

test('session, tournament and verified filters use the same conventions as the active view', () => {
    const selected = archiveOptions(summary()).filter((o) => o.team === 'eq118')
    const rows = [game('1', 'same', 1520), game('2', 'same', 1540, { session: 's2' }),
        game('3', 'same', 1600, { tournament: { id: 'cup' } }),
        game('4', 'same', 1700, { black: { team: 'remote', remote: true, all: [1500, 0] } })]
    assert.equal(archivedRows(rows, selected)[0].games, 3)
    assert.equal(archivedRows(rows, selected, { session: 's1' })[0].games, 3)
    assert.equal(archivedRows(rows, selected, { verified: true })[0].games, 2)
    assert.equal(archivedRows(rows, selected, { session: 's2' })[0].rating, 1440)
})

test('selected hashes cannot leak another team that used the same bytes', () => {
    const selected = archiveOptions(summary()).filter((o) => o.team === 'eq110')
    assert.deepEqual(archivedRows([game('1', 'same', 1520)], selected), [])
})


test('several archived versions of the same team can be selected independently', () => {
    const s = summary()
    s.builds.eq118.push('third')
    s.scopes[''].ratings[1] = { team: 'eq118', build: 'third', last: '4' }
    const options = archiveOptions(s).filter((o) => o.team === 'eq118')
    assert.deepEqual(options.map((o) => o.version), [1, 2])
    const rows = [game('1', 'same', 1520), game('2', 'new118', 1540), game('3', 'third', 1560)]
    const both = archivedRows(rows, options)
    assert.deepEqual(both.map((r) => [r.build, r.games, r.rating]), [['same', 1, 1520], ['new118', 1, 1540]])
    assert.deepEqual(archivedRows(rows, [options[1]]).map((r) => r.build), ['new118'])
})
