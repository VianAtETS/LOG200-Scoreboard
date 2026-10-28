// Rangs de la page graphe, rejoués partie par partie depuis timeline.jsonl (format dans docs/formats.md).
// Un côté sans build est un bot de référence : il ne prend pas de rang aux équipes.

export const TOP = 10

// Équipes tracées par défaut : les TOP premières du classement, bots exclus.
export function topTeams(ratings, n = TOP) {
    return ratings.filter((r) => r.build).slice(0, n).map((r) => r.team)
}

// Rejoue `rows` (une session si `session` est donné, sinon toutes) et rend, à partir de la première
// partie jouée à `cutoff` (ms) ou après, une colonne par partie et la série de chaque équipe de `selected`.
// Un point n'est ajouté que si le rang, le rating ou le build change : la série se lit en escalier.
// Rang d'une équipe : 1 + équipes devant elle (rating plus haut, puis nom). Un bot se place entre deux
// équipes : 0,5 + équipes devant lui.
export function rankSeries(rows, { key = 'all', session = '', selected = [], cutoff = -Infinity } = {}) {
    const ratings = new Map()
    const bots = new Set()
    const builds = new Map()
    const series = new Map(selected.map((team) => [team, []]))
    const columns = []
    for (const g of rows) {
        if (session && g.session !== session) continue
        const changed = new Map()
        for (const side of [g.red, g.black]) {
            ratings.set(side.team, side[key][0])
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
            const rank = rankOf(team, rating, ratings, bots)
            const last = points.at(-1)
            const version = changed.has(team)
            if (last && last.rank === rank && last.rating === rating && !version) continue
            points.push({ x, rank, rating, build: builds.get(team), version, bot: bots.has(team) })
        }
    }
    return { columns, series }
}

function rankOf(team, rating, ratings, bots) {
    let ahead = 0
    for (const [other, r] of ratings) {
        if (other === team || bots.has(other)) continue
        if (r > rating || (r === rating && other < team)) ahead++
    }
    return bots.has(team) ? ahead + 0.5 : ahead + 1
}

// Point de la série affiché à la colonne `x` : le dernier à x ou avant, `undefined` avant la branche.
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

// Rang moyen par paquet quand `count` colonnes ne tiennent pas en `buckets` : un point par paquet,
// au milieu des colonnes qu'il couvre (à partir du départ de la branche), à relier en diagonale.
// Sinon la série telle quelle.
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
