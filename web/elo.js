// Le classement Elo est calcule par le ladder (src/bin/ladder/standings.rs).

export const START = 1500

// Lignes illisibles (dont une derniere ligne en cours d'ecriture) et versions inconnues ignorees.
export function parseJsonl(text) {
    const games = []
    let skipped = 0
    for (const line of text.split('\n')) {
        if (!line.trim()) continue
        try {
            const g = JSON.parse(line)
            if (g.v === 1) games.push(g)
            else skipped++
        } catch {
            skipped++
        }
    }
    return { games, skipped }
}

// Historique d'une equipe a partir de `cutoff` (ms) : rating juste avant, puis les entrees suivantes.
export function since(history, cutoff) {
    const i = history.findIndex((h) => Date.parse(h.ts) >= cutoff)
    const first = i === -1 ? history.length : i
    return { start: history[first - 1]?.rating ?? START, points: history.slice(first) }
}

export const BETWEEN = 'entre'

// [rating après la partie, variation] d'un côté, sous `key` (all ou session). `verified` : sans les
// parties distantes, que le ladder n'écrit que s'ils diffèrent.
export function ratingOf(side, key, verified = false) {
    return (verified && side.verified?.[key]) || side[key]
}

export const isRemote = (g) => Boolean(g.red.remote || g.black.remote)

// Parties des équipes `selected` dans l'ordre, une seule fois chacune : une partie entre deux équipes
// choisies est dans leurs deux fichiers. BETWEEN : seulement les parties entre équipes choisies.
export function gamesOf(rowsByTeam, selected, { mode = '', session = '' } = {}) {
    const chosen = new Set(selected)
    const between = mode === BETWEEN && chosen.size > 1
    const games = new Map()
    for (const team of chosen) {
        for (const g of rowsByTeam.get(team) ?? []) {
            if (session && g.session !== session) continue
            if (between && !(chosen.has(g.red.team) && chosen.has(g.black.team))) continue
            games.set(g.id, g)
        }
    }
    return [...games.values()].sort((a, b) => (a.ts < b.ts ? -1 : a.ts > b.ts ? 1 : 0))
}

export function versionStarts(history) {
    const starts = []
    for (let k = 1; k < history.length; k++) {
        if (history[k].build && history[k].build !== history[k - 1].build) starts.push(k)
    }
    return starts
}
