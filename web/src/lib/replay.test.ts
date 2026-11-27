import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { TDescriptor } from '../types/game.types.ts'
import { ISOLATION, cellName, usable } from './game.ts'
import { BLACK, DESTROYED, EMPTY, RED, positions, squareIndex } from './replay.ts'

test('stored effects rebuild an Isolation game exactly like its moves', () => {
    const game = { opening: ['G1-G4-D7'], moves: ['F12-F9-G2'] }
    const fromMoves = positions(game)
    const changes = fromMoves.slice(1).map((s) => s.changes)
    const fromChanges = positions({ ...game, changes })
    assert.deepEqual(fromChanges.map((s) => s.cells), fromMoves.map((s) => s.cells))
    assert.deepEqual(fromChanges[1].move, fromMoves[1].move, 'le coup reste lisible')
    assert.deepEqual(
        fromMoves[2].changes.map((c) => c.role),
        ['from', 'to', 'effect'],
    )
    const last = fromChanges[2].cells
    assert.equal(last[squareIndex('F', 9)], BLACK)
    assert.equal(last[squareIndex('G', 4)], RED)
    assert.equal(last[squareIndex('F', 12)], EMPTY)
    assert.equal(last[squareIndex('G', 2)], DESTROYED)
})

// Un jeu 3 × 3 inconnu de la page : seuls ses effets le rejouent.
const TOY: TDescriptor = {
    game: 'toy',
    version: 1,
    name: 'Toy',
    rows: 3,
    cols: 3,
    sides: { red: { label: 'Attaquants' }, black: { label: 'Défenseurs' } },
    initial: [0, 0, 0, 0, 2, 0, 0, 0, 0],
    pieces: [
        { code: 0, label: 'vide', style: 'empty' },
        { code: 1, label: 'attaquant', style: 'pawn', side: 'red' },
        { code: 2, label: 'roi', style: 'king', side: 'black' },
    ],
}

test('any game replays from its descriptor and effects, without its rules', () => {
    assert.ok(usable(TOY))
    const steps = positions(
        {
            moves: ['A3', 'B2-B3'],
            changes: [
                [{ cell: 0, code: 1, role: 'to' }],
                [
                    { cell: 4, code: 0, role: 'from' },
                    { cell: 1, code: 2, role: 'to' },
                    { cell: 0, code: 0, role: 'effect' },
                ],
            ],
        },
        TOY,
    )
    assert.equal(steps.length, 3)
    assert.deepEqual(steps[2].cells, [0, 2, 0, 0, 0, 0, 0, 0, 0])
    assert.equal(steps[2].move, null, 'pas de notation d\'Isolation ici')
    assert.equal(steps[2].text, 'B2-B3')
    assert.equal(cellName(TOY, 1), 'B3')
    // Sans effets, un jeu inconnu ne se rejoue pas : seule la position initiale.
    assert.equal(positions({ moves: ['A3'] }, TOY).length, 1)
})

test('an unusable descriptor is refused, Isolation is the fallback', () => {
    assert.ok(usable(ISOLATION))
    assert.ok(!usable({ ...TOY, initial: [0] }))
    assert.ok(!usable(null))
    assert.equal(cellName(ISOLATION, 138), 'G1')
})
