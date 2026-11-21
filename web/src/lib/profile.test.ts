import { test } from 'node:test'
import assert from 'node:assert/strict'

import { callTree, topMethods } from './profile.ts'

test("l'arbre d'appels somme les chemins et range les branches par total, comme treeRows d'isola", () => {
    const root = callTree({ 'Client.main': 10, 'Client.main>Ia.search': 28, 'Client.main>Ia.search>Ia.eval': 2 }, 40)
    assert.deepEqual(root.kids.map((n) => [n.name, n.self, n.total]), [['Client.main', 10, 40]])
    const [search] = root.kids[0].kids
    assert.deepEqual([search.path, search.self, search.total], ['Client.main>Ia.search', 28, 30])
    assert.deepEqual(search.kids.map((n) => [n.path, n.self, n.total, n.kids.length]), [['Client.main>Ia.search>Ia.eval', 2, 2, 0]])
})

test('les branches sous 0,5 % des échantillons sont regroupées', () => {
    const root = callTree({ 'A.f': 995, 'A.f>B.g': 3, 'A.f>C.h': 2, 'D.k': 1 }, 1000)
    assert.deepEqual(root.kids.map((n) => n.name), ['A.f'])
    assert.equal(root.hidden, 1)
    assert.deepEqual([root.kids[0].kids.length, root.kids[0].hidden], [0, 2])
})

test('la liste garde les 20 méthodes au plus de temps propre', () => {
    const methods = Object.fromEntries(Array.from({ length: 25 }, (_, i) => [`M.m${i}`, [i, 30] as [number, number]]))
    const top = topMethods(methods)
    assert.equal(top.length, 20)
    assert.deepEqual(top[0], { name: 'M.m24', path: 'M.m24', self: 24, total: 30, kids: [], hidden: 0 })
    assert.equal(top.at(-1)!.self, 5)
})
