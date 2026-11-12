// Les versions restent celles de summary.builds : équipe + hash, jamais le seul nom d'équipe.
import { isRemote, ratingOf } from './elo.js'

export const archiveKey = (team, build) => `${encodeURIComponent(team)}@${build}`

export function currentBuilds(summary) {
    const latest = new Map()
    for (const scope of Object.values(summary.scopes)) for (const r of scope.ratings) {
        if (r.build && (!latest.has(r.team) || r.last > latest.get(r.team).last)) latest.set(r.team, r)
    }
    return new Map(Object.entries(summary.builds).map(([team, builds]) => [team, latest.get(team)?.build ?? builds.at(-1)]))
}

export function archiveOptions(summary) {
    const current = currentBuilds(summary)
    return Object.entries(summary.builds).flatMap(([team, builds]) => builds.flatMap((build, i) =>
        build === current.get(team) ? [] : [{ team, build, version: i + 1, key: archiveKey(team, build), label: `${team} v${i + 1}` }]))
}

// Dernier Elo réellement enregistré pour ce build, sans recalculer le classement actuel.
export function archivedRows(rows, selected, { session = '', verified = false } = {}) {
    const wanted = new Map(selected.map((s) => [s.key, s]))
    const out = new Map()
    for (const g of rows) {
        if (session ? g.session !== session : g.tournament) continue
        if (verified && isRemote(g)) continue
        for (const color of ['red', 'black']) {
            const side = g[color]
            const id = archiveKey(side.team, side.build)
            if (!wanted.has(id)) continue
            const [rating, delta] = ratingOf(side, session ? 'session' : 'all', verified)
            const r = out.get(id) ?? { ...wanted.get(id), rating, peak: rating - delta, games: 0,
                wins: 0, losses: 0, build_games: 0, build_wins: 0, archived: true, remote: false }
            r.rating = rating
            r.peak = Math.max(r.peak, rating)
            r.games++
            r.build_games++
            if (g.winner === color) { r.wins++; r.build_wins++ } else r.losses++
            r.remote ||= Boolean(side.remote)
            r.last = g.ts
            out.set(id, r)
        }
    }
    return [...out.values()]
}
