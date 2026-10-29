// Rangs de la page graphe, rejoués partie par partie depuis timeline.jsonl (format dans docs/formats.md).
// Un côté sans build est un bot de référence : il ne compte dans le classement que s'il est choisi.

import { isRemote, ratingOf } from './elo.js'

export const TOP = 10

export function topTeams(ratings, n = TOP) {
    return ratings.filter((r) => r.build).slice(0, n).map((r) => r.team)
}

// Une colonne par partie jouée à `cutoff` (ms) ou après, et pour chaque équipe de `selected` un point à
// chaque changement de rang, de rating ou de build. Rang : 1 + équipes et bots choisis devant (rating
// plus haut, puis nom). `verified` : sans les parties distantes.
export function rankSeries(
    rows,
    { key = 'all', session = '', selected = [], cutoff = -Infinity, verified = false } = {}
) {
    const ratings = new Map()
    const bots = new Set()
    const counted = (team) => !bots.has(team) || series.has(team)
    const builds = new Map()
    const series = new Map(selected.map((team) => [team, []]))
    const columns = []
    for (const g of rows) {
        if ((session && g.session !== session) || (verified && isRemote(g))) continue
        const changed = new Map()
        for (const side of [g.red, g.black]) {
            ratings.set(side.team, ratingOf(side, key, verified)[0])
            if (!side.build) bots.add(side.team)
            const before = builds.get(side.team)
            if (side.build && side.build !== before) {
                builds.set(side.team, side.build)
                if (before) changed.set(side.team, side.build)
            }
        }
        if (Date.parse(g.ts) < cutoff) continue
        const x = columns.push(g) - 1
        for (const [team, points] of series) {
            const rating = ratings.get(team)
            if (rating === undefined) continue
            const rank = rankOf(team, rating, ratings, counted)
            const last = points.at(-1)
            const version = changed.has(team)
            if (last && last.rank === rank && last.rating === rating && !version) continue
            points.push({ x, rank, rating, build: builds.get(team), version, bot: bots.has(team) })
        }
    }
    return { columns, series }
}

function rankOf(team, rating, ratings, counted) {
    let ahead = 0
    for (const [other, r] of ratings) {
        if (other === team || !counted(other)) continue
        if (r > rating || (r === rating && other < team)) ahead++
    }
    return ahead + 1
}

// Dernier point à `x` ou avant, `undefined` avant le départ de la branche.
export function pointAt(points, x) {
    let lo = 0
    let hi = points.length
    while (lo < hi) {
        const mid = (lo + hi) >> 1
        if (points[mid].x <= x) lo = mid + 1
        else hi = mid
    }
    return points[lo - 1]
}

// Rang moyen par paquet quand `count` colonnes ne tiennent pas en `buckets`, sinon la série telle quelle.
export function smooth(points, count, buckets) {
    if (count <= buckets || !points.length) return points
    const out = []
    const size = count / buckets
    let i = 0
    for (let b = 0; b < buckets; b++) {
        const end = Math.round((b + 1) * size)
        const start = Math.max(Math.round(b * size), points[0].x)
        if (start >= end) continue
        let sum = 0
        for (let col = start; col < end;) {
            while (i + 1 < points.length && points[i + 1].x <= col) i++
            const next = Math.min(points[i + 1]?.x ?? end, end)
            sum += points[i].rank * (next - col)
            col = next
        }
        out.push({ x: (start + end - 1) / 2, rank: sum / (end - start) })
    }
    return out
}
