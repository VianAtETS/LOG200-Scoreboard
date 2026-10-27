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

export function versionStarts(history) {
    const starts = []
    for (let k = 1; k < history.length; k++) {
        if (history[k].build && history[k].build !== history[k - 1].build) starts.push(k)
    }
    return starts
}
