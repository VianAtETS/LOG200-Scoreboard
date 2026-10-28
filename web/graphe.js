// Page graphe : rang des équipes choisies après chaque partie (bump chart), rejoué depuis
// timeline.jsonl ; un nœud marque chaque nouvelle version de jar.
// ?session=, ?teams= et ?window= suivent les choix de la page : l'adresse se partage telle quelle.

import { pointAt, rankSeries, smooth, topTeams } from './bump.js'
import { createPicker } from './picker.js'
import {
    $,
    DATA,
    dayFormat,
    el,
    fillSelect,
    fullFormat,
    GLOBAL,
    linkPages,
    pageUrl,
    params,
    pulse,
    REFRESH_MS,
    setParam,
    tail,
    timeFormat,
    WINDOWS,
} from './common.js'

const SESSION = params.get('session')
const WINDOW = params.get('window') ?? GLOBAL
// Couleurs --series-1 à --series-10, répétées au-delà.
const COLORS = 10
const SVG = 'http://www.w3.org/2000/svg'
const MARGIN = { top: 18, right: 96, bottom: 28, left: 36 }
// Pixels par partie en dessous desquels le rang est lissé, et largeur d'un paquet lissé.
const EXACT_PX = 4
const BUCKET_PX = 12

let summary = null
let summaryText = null
let timeline = { offset: null, rows: [] }
const colors = new Map()
let builds = new Map()
// Derniers rangs calculés : un redimensionnement redessine sans rejouer la timeline.
let view = null
// null : le top 10, qui suit le classement.
let chosen = params.has('teams') ? params.get('teams').split(',').filter(Boolean) : null
// `armed` : au doigt, un premier appui montre la partie, un second sur la même colonne l'ouvre.
let hover = { col: null, team: null, armed: null, touch: false }

async function refresh() {
    try {
        const res = await fetch(`${DATA}/summary.json`, { cache: 'no-store' })
        if (!res.ok) throw new Error(`HTTP ${res.status}`)
        const text = await res.text()
        const first = summary === null
        let changed = text !== summaryText
        if (changed) {
            summaryText = text
            summary = JSON.parse(text)
            builds = new Map(Object.entries(summary.builds))
            fillSelect($('session'), summary.sessions, first ? (SESSION ?? summary.sessions[0]) : $('session').value)
        }
        changed = (await tail(`${DATA}/timeline.jsonl`, timeline)) || changed
        if (changed || first) render()
        if (changed && !first) pulse()
        $('live').className = 'live on'
    } catch (e) {
        $('live').className = 'live error'
        $('status').textContent = `Données indisponibles : ${e.message}`
    }
}

function selectedTeams(ratings) {
    if (chosen === null) return topTeams(ratings)
    const known = new Set(ratings.map((r) => r.team))
    return chosen.filter((t) => known.has(t))
}

// Première couleur libre ; les équipes retirées libèrent la leur.
function colorOf(team, selected) {
    for (const t of colors.keys()) if (!selected.includes(t)) colors.delete(t)
    if (!colors.has(team)) {
        const used = new Set(colors.values())
        let slot = 0
        while (used.has(slot) && slot < COLORS) slot++
        colors.set(team, slot < COLORS ? slot : colors.size % COLORS)
    }
    return `var(--series-${colors.get(team) + 1})`
}

function version(team, build) {
    const n = (builds.get(team)?.indexOf(build) ?? -1) + 1
    return n ? `v${n}` : ''
}

function svg(tag, attrs = {}, text) {
    const node = document.createElementNS(SVG, tag)
    for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v)
    if (text !== undefined) node.textContent = text
    return node
}

function render() {
    if (!summary) return
    const session = $('session').value
    const scope = summary.scopes[session]
    const selected = selectedTeams(scope.ratings)
    const w = document.querySelector('#bump-window input:checked')?.value
    const { columns, series } = rankSeries(timeline.rows, {
        key: session === GLOBAL ? 'all' : 'session',
        session,
        selected,
        cutoff: Date.now() - (WINDOWS[w] ?? Infinity),
    })
    // Un bot reste gris et ne prend pas de couleur.
    const bots = new Set(scope.ratings.filter((r) => !r.build).map((r) => r.team))
    const palette = new Map(selected.filter((t) => !bots.has(t)).map((t) => [t, colorOf(t, selected)]))
    view = { columns, series, selected, palette }
    view.geom = draw(view)
    if (hover.col !== null) showAt(hover.col)
    picker.render(selected, {
        color: (team) => palette.get(team) ?? 'var(--muted)',
        idle: (team) => !series.get(team)?.length,
    })
    $('status').textContent = `${columns.length} parties dans la fenêtre, à jour à ${timeFormat.format(new Date())}`
}

// Rend la géométrie du graphe pour l'infobulle, null si vide.
function draw({ columns, series, selected, palette }) {
    const box = $('bump-plot').getBoundingClientRect()
    const width = Math.max(240, box.width)
    const height = Math.max(200, box.height)
    const plot = $('bump-svg')
    plot.setAttribute('viewBox', `0 0 ${width} ${height}`)
    const drawn = selected.filter((t) => series.get(t)?.length)
    if (!columns.length || !drawn.length) {
        plot.replaceChildren()
        $('bump-summary').textContent = !selected.length
            ? 'Aucune équipe choisie.'
            : 'Aucune partie de ces équipes dans cette fenêtre.'
        return null
    }

    const ranks = drawn.flatMap((t) => series.get(t).map((p) => p.rank))
    const top = Math.floor(Math.min(...ranks))
    const bottom = Math.ceil(Math.max(...ranks))
    const inner = { w: width - MARGIN.left - MARGIN.right, h: height - MARGIN.top - MARGIN.bottom }
    const x = (col) => MARGIN.left + (columns.length > 1 ? (col / (columns.length - 1)) * inner.w : inner.w / 2)
    const y = (rank) => MARGIN.top + (bottom > top ? ((rank - top) / (bottom - top)) * inner.h : inner.h / 2)

    const grid = svg('g', { class: 'bump-grid' })
    const step = Math.ceil((bottom - top + 1) / Math.max(1, Math.floor(inner.h / 18)))
    for (let r = top; r <= bottom; r++) {
        grid.append(svg('line', { x1: MARGIN.left, x2: MARGIN.left + inner.w, y1: y(r), y2: y(r) }))
        if ((r - top) % step === 0) grid.append(svg('text', { x: MARGIN.left - 8, y: y(r), class: 'rank' }, String(r)))
    }
    grid.append(...dateTicks(columns, x, height, inner.w))

    const lines = svg('g', { class: 'bump-lines' })
    const nodes = svg('g', { class: 'bump-nodes' })
    const labels = svg('g', { class: 'bump-labels' })
    const isBot = (team) => series.get(team)[0].bot
    const buckets = columns.length * EXACT_PX <= inner.w ? columns.length : Math.floor(inner.w / BUCKET_PX)
    const paths = new Map()
    // Les lignes du haut du classement sont dessinées en dernier, au-dessus des autres.
    for (const team of [...drawn].reverse()) {
        const points = series.get(team)
        const path = vertices(points, smooth(points, columns.length, buckets), columns.length)
        paths.set(team, path)
        const color = palette.get(team) ?? 'var(--muted)'
        const bot = isBot(team)
        const d = path.map(([col, rank], i) => `${i ? 'L' : 'M'}${x(col)},${y(rank)}`).join('')
        lines.append(svg('path', { d, class: bot ? 'bot' : '', style: `stroke: ${color}`, 'data-team': team }))
        // Départ de branche dans la fenêtre.
        const branch = { cx: x(path[0][0]), cy: y(path[0][1]), r: 3, style: `fill: ${color}`, 'data-team': team }
        if (points[0].x > 0) nodes.append(svg('circle', branch))
        for (const p of points.filter((p) => p.version)) {
            const cy = y(rankAt(path, p.x))
            const node = { cx: x(p.x), cy, r: 5, class: 'version', style: `fill: ${color}`, 'data-team': team }
            nodes.append(svg('circle', node))
            const text = { x: x(p.x), y: cy - 9, class: 'version-label', 'data-team': team }
            nodes.append(svg('text', text, version(team, p.build)))
        }
        const label = svg('g', {
            transform: `translate(${MARGIN.left + inner.w + 8},${y(path.at(-1)[1])})`,
            'data-team': team,
        })
        label.append(
            svg('circle', { r: 4, style: `fill: ${color}` }),
            svg('text', { x: 8 }, team)
        )
        labels.append(label)
    }
    plot.replaceChildren(grid, lines, nodes, labels)

    const from = dayFormat.format(new Date(columns[0].ts))
    const to = dayFormat.format(new Date(columns.at(-1).ts))
    const parts = [`Rang de ${drawn.length} équipes sur ${columns.length} parties, du ${from} au ${to}`]
    const teams = drawn.filter((t) => !series.get(t)[0].bot)
    if (teams.length) {
        const rank = (t) => series.get(t).at(-1).rank
        const leader = teams.reduce((a, b) => (rank(a) <= rank(b) ? a : b))
        parts.push(`en tête des équipes suivies : ${leader}, ${ordinal(rank(leader))}`)
    }
    $('bump-summary').textContent = `${parts.join(' ; ')}.`
    $('bump-plot').setAttribute('aria-valuemax', String(columns.length - 1))
    return { x, y, inner, width, drawn, paths }
}

// Sommets [colonne, rang] de la ligne. Rang exact : palier, puis diagonale sur une partie.
function vertices(points, line, count) {
    const path = []
    if (line === points) {
        points.forEach((p, i) => {
            if (i) path.push([Math.max(p.x - 1, points[i - 1].x), points[i - 1].rank])
            path.push([p.x, p.rank])
        })
    } else {
        path.push([points[0].x, line[0].rank], ...line.map((p) => [p.x, p.rank]))
    }
    path.push([count - 1, path.at(-1)[1]])
    return path
}

// Rang interpolé de la ligne à `col` : un nœud se pose sur la ligne, même lissée.
function rankAt(path, col) {
    const k = path.findIndex(([c]) => c >= col)
    if (k <= 0) return path[Math.max(k, 0)][1]
    const [[c0, r0], [c1, r1]] = [path[k - 1], path[k]]
    return c1 === c0 ? r1 : r0 + ((r1 - r0) * (col - c0)) / (c1 - c0)
}

const ordinal = (rank) => (rank === 1 ? '1re' : `${rank}e`)

// Cinq dates au plus, une par 100 px.
function dateTicks(columns, x, height, width) {
    const span = Date.parse(columns.at(-1).ts) - Date.parse(columns[0].ts)
    const format = span < 864e5 ? timeFormat : dayFormat
    const count = Math.min(5, columns.length, Math.max(2, Math.floor(width / 100)))
    const ticks = []
    let previous = ''
    for (let k = 0; k < count; k++) {
        const col = count > 1 ? Math.round((k * (columns.length - 1)) / (count - 1)) : 0
        const text = format.format(new Date(columns[col].ts))
        if (text === previous) continue
        previous = text
        const anchor = k === 0 ? 'start' : k === count - 1 ? 'end' : 'middle'
        ticks.push(svg('text', { x: x(col), y: height - 8, class: 'date', 'text-anchor': anchor }, text))
    }
    return ticks
}

const plot = $('bump-plot')

// Rang exact, pas lissé, meilleure d'abord.
function standingsAt(col) {
    const { series, geom } = view
    return geom.drawn
        .map((team) => ({ team, p: pointAt(series.get(team), col), before: pointAt(series.get(team), col - 1) }))
        .filter((r) => r.p)
        .sort((a, b) => a.p.rank - b.p.rank)
}

function showAt(col, team = hover.team) {
    const { columns, palette, geom } = view ?? {}
    if (!geom) return hideTip()
    col = Math.max(0, Math.min(columns.length - 1, col))
    const rows = standingsAt(col)
    if (!rows.some((r) => r.team === team)) team = rows[0]?.team ?? null
    hover.col = col
    hover.team = team

    const left = geom.x(col)
    Object.assign($('bump-cursor').style, { left: `${left}px`, top: `${MARGIN.top}px`, height: `${geom.inner.h}px` })
    $('bump-cursor').hidden = false
    $('bump-svg').classList.toggle('focusing', Boolean(team))
    for (const node of $('bump-svg').querySelectorAll('[data-team]')) node.classList.toggle('on', node.dataset.team === team)

    const g = columns[col]
    const winner = g.red.all[1] >= g.black.all[1] ? g.red.team : g.black.team
    const loser = winner === g.red.team ? g.black.team : g.red.team
    const tip = $('bump-tip')
    tip.replaceChildren(
        el('p', { class: 'tip-when' }, `${fullFormat.format(new Date(g.ts))} · partie ${col + 1} sur ${columns.length}`),
        el('p', { class: 'tip-game' }, el('strong', {}, winner), ` bat ${loser}`),
        el(
            'ol',
            { class: 'tip-rows' },
            ...rows.map(({ team: t, p, before }) => {
                const dot = el('span', { class: 'chip-dot' })
                dot.style.background = palette.get(t) ?? 'var(--muted)'
                const move = before && before.rank !== p.rank ? (p.rank < before.rank ? ' ▲' : ' ▼') : ''
                const played = t === g.red.team || t === g.black.team
                return el(
                    'li',
                    { class: `${t === team ? 'on' : ''}${played ? ' played' : ''}` },
                    dot,
                    el('span', { class: 'tip-rank' }, p.bot ? 'bot' : `${ordinal(p.rank)}${move}`),
                    el('span', { class: 'tip-team' }, [t, version(t, p.build)].filter(Boolean).join(' ')),
                    el('span', { class: 'tip-elo' }, `${Math.round(p.rating)} Elo`)
                )
            })
        ),
        el('p', { class: 'tip-hint' }, hover.touch ? 'Touchez encore pour rejouer la partie' : 'Clic ou Entrée : rejouer la partie')
    )
    tip.hidden = false
    const width = tip.offsetWidth
    tip.style.left = `${left + 14 + width <= geom.width ? left + 14 : Math.max(0, left - 14 - width)}px`

    const focus = rows.find((r) => r.team === team)
    const reading = focus ? `${team} ${focus.p.bot ? 'bot' : ordinal(focus.p.rank)}, ${Math.round(focus.p.rating)} Elo, ` : ''
    plot.setAttribute('aria-valuenow', String(col))
    plot.setAttribute('aria-valuetext', `${reading}${fullFormat.format(new Date(g.ts))}, ${winner} bat ${loser}`)
}

function hideTip() {
    hover = { ...hover, col: null, armed: null }
    $('bump-tip').hidden = true
    $('bump-cursor').hidden = true
    $('bump-svg').classList.remove('focusing')
}

function pointed(e) {
    const { geom, columns } = view
    const box = plot.getBoundingClientRect()
    const px = e.clientX - box.left
    const py = e.clientY - box.top
    const col = Math.round(((px - MARGIN.left) / geom.inner.w) * (columns.length - 1))
    const at = Math.max(0, Math.min(columns.length - 1, col))
    let team = null
    let best = Infinity
    for (const [t, path] of geom.paths) {
        if (path[0][0] > at) continue
        const distance = Math.abs(geom.y(rankAt(path, at)) - py)
        if (distance < best) [team, best] = [t, distance]
    }
    return { col: at, team }
}

// Filtré sur l'équipe visée si elle joue la partie, sinon sur les équipes choisies.
function openGame(col) {
    const g = view.columns[col]
    const team = [g.red.team, g.black.team].includes(hover.team) ? hover.team : ''
    location.href = pageUrl('./', { teams: team, game: g.id })
}

plot.addEventListener('pointermove', (e) => {
    if (!view?.geom) return
    hover.touch = e.pointerType === 'touch'
    const { col, team } = pointed(e)
    showAt(col, team)
})
plot.addEventListener('pointerdown', (e) => (hover.touch = e.pointerType === 'touch'))
plot.addEventListener('pointerleave', () => {
    if (document.activeElement !== plot) hideTip()
})
plot.addEventListener('click', (e) => {
    if (!view?.geom) return
    const { col, team } = pointed(e)
    if (hover.touch && hover.armed !== col) {
        hover.armed = col
        return showAt(col, team)
    }
    openGame(col)
})
plot.addEventListener('focus', () => view?.geom && showAt(hover.col ?? view.columns.length - 1))
plot.addEventListener('blur', hideTip)
plot.addEventListener('keydown', (e) => {
    if (!view?.geom || hover.col === null) return
    const page = Math.max(1, Math.round(view.columns.length / 10))
    const moves = {
        ArrowLeft: hover.col - 1,
        ArrowRight: hover.col + 1,
        PageUp: hover.col - page,
        PageDown: hover.col + page,
        Home: 0,
        End: view.columns.length - 1,
    }
    if (e.key in moves) showAt(moves[e.key])
    else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
        const rows = standingsAt(hover.col)
        const i = rows.findIndex((r) => r.team === hover.team) + (e.key === 'ArrowUp' ? -1 : 1)
        showAt(hover.col, rows[Math.max(0, Math.min(rows.length - 1, i))]?.team)
    } else if (e.key === 'Enter' || e.key === ' ') openGame(hover.col)
    else if (e.key === 'Escape') hideTip()
    else return
    e.preventDefault()
})

// null : retour au haut du classement (?teams= retiré) ; [] : aucune équipe (?teams= vide).
function choose(teams) {
    chosen = teams
    setParam('teams', teams?.join(','), true)
    render()
}

const picker = createPicker({
    ratings: () => (summary ? summary.scopes[$('session').value].ratings : []),
    onChange: choose,
})
$('picker-top').addEventListener('click', () => choose(null))

for (const radio of document.querySelectorAll('#bump-window input'))
    radio.checked = radio.value === (WINDOW in WINDOWS ? WINDOW : GLOBAL)
$('bump-window').addEventListener('change', (e) => {
    setParam('window', e.target.value)
    render()
})
$('session').addEventListener('change', () => {
    setParam('session', $('session').value, true)
    render()
})
new ResizeObserver(() => {
    if (!view) return
    view.geom = draw(view)
    if (hover.col !== null) showAt(hover.col)
}).observe($('bump-plot'))
linkPages()

refresh()
setInterval(refresh, REFRESH_MS)
