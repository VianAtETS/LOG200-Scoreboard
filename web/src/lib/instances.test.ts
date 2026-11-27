import assert from 'node:assert/strict'
import { test } from 'node:test'

import { instanceUrls, parseInstances } from './instances.ts'

test('instances.json keeps only the usable instances', () => {
    const file = {
        v: 1,
        instances: [
            { id: 'isolation', label: 'Automne 2026' },
            { id: 'hnefatafl', archived: true },
            { id: '../etc' },
            { id: 'Majuscules' },
            { id: 'ok', label: 3 },
            null,
        ],
    }
    assert.deepEqual(parseInstances(file), [
        { id: 'isolation', label: 'Automne 2026' },
        { id: 'hnefatafl', archived: true },
    ])
    assert.equal(parseInstances({ v: 2, instances: [] }), null)
    assert.equal(parseInstances(null), null)
    assert.deepEqual(instanceUrls('..', 'isolation'), { page: '../isolation/web/', data: '../isolation/data' })
})
