import assert from 'node:assert/strict'
import test from 'node:test'

import { releaseState } from './version.js'

test('the ladder matches the latest release tag', () => {
    assert.equal(releaseState('0.24.1', 'v0.24.1'), 'current')
})

test('an older ladder is behind, whatever the field', () => {
    assert.equal(releaseState('0.24.1', 'v0.24.2'), 'behind')
    assert.equal(releaseState('0.24.9', 'v0.25.0'), 'behind')
    assert.equal(releaseState('0.9.0', 'v0.10.0'), 'behind')
    assert.equal(releaseState('0.99.0', 'v1.0.0'), 'behind')
})

test('a ladder built after the latest release is ahead', () => {
    assert.equal(releaseState('0.25.0', 'v0.24.1'), 'ahead')
})

test('a missing or malformed version is unknown', () => {
    assert.equal(releaseState('0.24.1', undefined), 'unknown')
    assert.equal(releaseState('', 'v0.24.1'), 'unknown')
    assert.equal(releaseState('0.24', 'v0.24.1'), 'unknown')
    assert.equal(releaseState('0.24.x', 'v0.24.1'), 'unknown')
})
