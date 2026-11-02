const parse = (version) => {
    const parts = (version ?? '').replace(/^v/, '').split('.')
    return parts.length === 3 && parts.every((p) => /^\d+$/.test(p)) ? parts.map(Number) : null
}

export function releaseState(ladder, latestTag) {
    const [mine, latest] = [parse(ladder), parse(latestTag)]
    if (!mine || !latest) return 'unknown'
    const differs = mine.findIndex((part, i) => part !== latest[i])
    if (differs < 0) return 'current'
    return mine[differs] < latest[differs] ? 'behind' : 'ahead'
}
