import { $, DATA, el, params, setParam, tail } from './common.js'
import { archiveOptions, archivedRows, currentBuilds } from './archives.js'

export function createArchiveFilter(onChange) {
    const box = $('archive-filter')
    const title = el('summary', {}, 'Versions archivées')
    const search = el('input', { type: 'search', 'aria-label': 'Filtrer les archives par équipe, version ou hash', placeholder: 'Équipe, version ou hash…' })
    const clear = el('button', { type: 'button' }, 'Tout masquer')
    const list = el('fieldset', {}, el('legend', {}, 'Versions à afficher'))
    const status = el('p', { class: 'archive-status', role: 'status' })
    box.append(title, el('div', { class: 'archive-tools' }, search, clear), list,
        el('p', { class: 'archive-help' }, 'Archives : anciennes versions, sans nouveaux matchs. Elo figé au dernier match de la version ; hors classement actuel.'), status)
    let enabled = false
    let options = []
    let current = new Map()
    let signature = ''
    const wanted = new Set((params.get('archives') ?? '').split(',').filter(Boolean))
    const states = new Map()
    let loading = Promise.resolve()
    const selected = () => enabled ? options.filter((o) => wanted.has(o.key)) : []

    function render() {
        title.textContent = `Versions archivées (${selected().length})`
        const query = search.value.trim().toLowerCase()
        const rows = options.filter((o) => `${o.label} ${o.build}`.toLowerCase().includes(query))
        const next = JSON.stringify([rows, [...wanted]])
        if (next === signature) return
        signature = next
        const focused = list.contains(document.activeElement) ? document.activeElement.value : null
        list.replaceChildren(el('legend', {}, 'Versions à afficher'), ...rows.map((o) => {
            const input = el('input', { type: 'checkbox', value: o.key })
            input.checked = wanted.has(o.key)
            input.addEventListener('change', () => {
                if (input.checked) wanted.add(o.key)
                else wanted.delete(o.key)
                setParam('archives', [...wanted].join(','))
                render()
                onChange()
            })
            return el('label', { title: `JAR ${o.build}` }, input, `${o.label} · ${o.build.slice(0, 8)}`, el('span', { class: 'badge archived' }, 'Archivé'))
        }), ...(rows.length ? [] : [el('p', {}, 'Aucune version archivée ne correspond.')]))
        if (focused) [...list.querySelectorAll('input')].find((input) => input.value === focused)?.focus({ preventScroll: true })
    }

    function reset() {
        wanted.clear()
        setParam('archives', null)
        render()
    }
    search.addEventListener('input', render)
    clear.addEventListener('click', () => { reset(); onChange() })

    return {
        selected,
        isArchived: (team, build) => enabled && Boolean(build) && current.has(team) && build !== current.get(team),
        update(summary) {
            enabled = summary.archives === true
            current = currentBuilds(summary)
            options = enabled ? archiveOptions(summary) : []
            box.hidden = !enabled
            if (enabled) render()
        },
        load() {
            const teams = new Set(selected().map((o) => o.team))
            for (const team of states.keys()) if (!teams.has(team)) states.delete(team)
            for (const team of teams) if (!states.has(team)) states.set(team, { offset: null, rows: [] })
            const entries = [...states]
            loading = loading.catch(() => {}).then(async () => {
                try {
                    await Promise.all(entries.map(([team, state]) => tail(`${DATA}/teams/${encodeURIComponent(team)}.jsonl`, state)))
                    status.textContent = ''
                } catch (error) {
                    status.textContent = `Archives indisponibles : ${error.message}`
                }
            })
            return loading
        },
        rows(options) {
            const games = new Map([...states.values()].flatMap((s) => s.rows.map((g) => [g.id, g])))
            return archivedRows([...games.values()].sort((a, b) => a.id.localeCompare(b.id)), selected(), options)
        },
    }
}
