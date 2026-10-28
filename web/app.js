// Page du ladder : classement et dernières parties de summary.json, parties des équipes choisies
// dans teams/, replay lu partie par partie dans games.jsonl.
// ?session=, ?teams=, ?match=, ?window= et ?game= suivent les choix de la page : l'adresse se partage
// telle quelle. ?team= des anciens liens vaut ?teams= avec cette seule équipe.

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
    roundDelta,
    SERVER,
    setParam,
    signed,
    tail,
    timeFormat,
    WINDOWS,
} from './common.js'
import { BETWEEN, gamesOf, since, versionStarts } from './elo.js'
import { createPicker } from './picker.js'
import { BLACK, DESTROYED, positions, RED, SIZE, squareName } from './replay.js'

const RECENT_GAMES = 50
const PLAY_MS = 500
// p99 au-delà de cette part du temps par coup : l'équipe frôle le timeout.
const PACE_WARN = 0.8
const SESSION = params.get('session')
const GAME = params.get('game')
const WINDOW = params.get('window') ?? GLOBAL

// Raison de la fin, du point de vue du perdant : liste compacte, puis phrase du replay.
const REASONS = {
    no_moves: 'bloqué',
    timeout: 'temps',
    disconnect: 'déconnexion',
    protocol: 'coups invalides',
    no_show: 'absent',
}
const ENDINGS = {
    no_moves: (loser) => `${loser} n'a plus de coup`,
    timeout: (loser) => `${loser} a dépassé son temps`,
    disconnect: (loser) => `${loser} s'est déconnecté`,
    protocol: (loser) => `${loser} a joué trop de coups invalides`,
    no_show: (loser) => `${loser} ne s'est pas connecté`,
}

let summary = null
let summaryText = null
// Aucune équipe choisie : les dernières parties de toutes les équipes.
let chosen = (params.get('teams') ?? params.get('team') ?? '').split(',').filter(Boolean)
// Équipe choisie vers l'état de `tail` de teams/<équipe>.jsonl.
const followed = new Map()
let following = Promise.resolve(false)
// Équipe vers ses builds dans l'ordre, toutes sessions : sert à afficher vN.
let builds = new Map()
// Courbe de l'équipe recherchée : points[0] est le départ à START, puis une entrée d'historique par partie.
let chart = { team: null, points: [], min: 0, max: 0, index: 0 }
// `pinned` : la personne a choisi une partie (ou un lien ?game=) ; sinon le replay suit la dernière jouée.
let replay = { game: null, steps: [], index: 0, timer: null, pinned: Boolean(GAME) }
// Dernier replay demandé : une réponse plus ancienne arrivée en retard est ignorée.
let replaySeq = 0

function sameDay(a, b) {
    return a.toDateString() === b.toDateString()
}

function shortWhen(ts) {
    const d = new Date(ts)
    return sameDay(d, new Date()) ? timeFormat.format(d) : dayFormat.format(d)
}

function ago(ts) {
    const minutes = Math.floor((Date.now() - new Date(ts)) / 60000)
    if (minutes < 1) return "à l'instant"
    if (minutes < 60) return `il y a ${minutes} min`
    if (minutes < 24 * 60) return `il y a ${Math.floor(minutes / 60)} h`
    return dayFormat.format(new Date(ts))
}

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
            renderSessions(first)
        }
        changed = (await follow()) || changed
        if (first && GAME) openLinked().catch(() => (replay.pinned = false))
        render()
        if (changed && !first) pulse()
        $('live').className = 'live on'
    } catch (e) {
        $('live').className = 'live error'
        $('status').textContent = `Données indisponibles : ${e.message}`
    }
}

const teamUrl = (team) => `${DATA}/teams/${encodeURIComponent(team)}.jsonl`
const allTeams = () => summary.scopes[GLOBAL].ratings.map((r) => r.team)

function selectedTeams() {
    if (!summary) return []
    const known = new Set(allTeams())
    return chosen.filter((t) => known.has(t))
}

// Lectures à la file : deux lectures en parallèle d'un même fichier doubleraient des lignes.
function follow() {
    const teams = selectedTeams()
    for (const team of followed.keys()) if (!teams.includes(team)) followed.delete(team)
    for (const team of teams) if (!followed.has(team)) followed.set(team, { offset: null, rows: [] })
    const states = [...followed]
    following = following
        .catch(() => { })
        .then(async () => (await Promise.all(states.map(([team, state]) => tail(teamUrl(team), state)))).some(Boolean))
    return following
}

const matchMode = () => document.querySelector('#match input:checked')?.value ?? GLOBAL

// Lien ?game= : l'id finit par `-<rouge>-<noir>`, la partie est dans le fichier de l'équipe rouge.
async function openLinked() {
    const pair = GAME.slice(GAME.indexOf('-') + 1)
    const teams = allTeams()
    const red = teams.find((t) => pair.startsWith(`${t}-`) && teams.includes(pair.slice(t.length + 1)))
    const state = { offset: null, rows: [] }
    if (red) await tail(teamUrl(red), state)
    const row = state.rows.find((g) => g.id === GAME)
    if (row) showGame(row, true)
    else replay.pinned = false
}

// Fondu en bas d'une zone tant qu'il reste du contenu à faire défiler.
function markOverflow(node) {
    node.classList.toggle('more', node.scrollTop + node.clientHeight < node.scrollHeight - 2)
}

const scrollAreas = [document.querySelector('.ranking-scroll'), $('games')]
for (const node of scrollAreas) node.addEventListener('scroll', () => markOverflow(node), { passive: true })
addEventListener('resize', () => scrollAreas.forEach(markOverflow))

function renderSessions(first) {
    fillSelect($('session'), summary.sessions, first ? (SESSION ?? summary.sessions[0]) : $('session').value)
    // Ancien lien ?team= : réécrit en ?teams=.
    if (first && params.has('team')) {
        setParam('team', null)
        setParam('teams', selectedTeams().join(','))
    }
}

function render() {
    if (!summary) return
    const session = $('session').value
    const scope = summary.scopes[session]
    // Colonne Elo des lignes : toutes sessions, ou la session choisie.
    const key = session === GLOBAL ? 'all' : 'session'
    const teams = selectedTeams()
    const rows = new Map([...followed].map(([team, state]) => [team, state.rows]))
    const list = teams.length ? gamesOf(rows, teams, { mode: matchMode(), session }) : scope.recent
    renderRanking(scope.ratings)
    picker.render(teams)
    $('match').hidden = teams.length < 2
    renderChart(scope.ratings, list, key, teams.length === 1 ? teams[0] : null)
    $('graph-link').hidden = teams.length < 2
    $('graph-link').href = pageUrl('graphe.html', { teams: teams.join(',') })
    renderGames(list, key)

    const time = scope.recent.at(-1)?.time_ms
    $('time-ms').textContent = time ?? '?'
    const pace = time ? `, ${time} ms par coup` : ''
    const count = teams.length ? list.length : scope.games
    $('status').textContent = `${count} parties${pace}, à jour à ${timeFormat.format(new Date())}`

    scrollAreas.forEach(markOverflow)

    const latest = list.at(-1)
    if (latest && !replay.pinned && replay.game?.id !== latest.id) showGame(latest)
}

// « v3 » pour le build de `player`, vide sans build.
function version(player) {
    const n = player.build ? (builds.get(player.team)?.indexOf(player.build) ?? -1) + 1 : 0
    return n ? `v${n}` : ''
}

function paceCell(pace) {
    if (!pace) return el('td', { class: 'num pace' })
    const title = `Médiane ${pace.p50} ms, p99 ${pace.p99} ms, max ${pace.max} ms, ${pace.count} coups`
    const warn = pace.p99 >= PACE_WARN * pace.limit ? ' warn' : ''
    return el('td', { class: `num pace${warn}`, title }, `${pace.p95} ms`)
}

function renderRanking(ratings) {
    $('ranking-empty').hidden = ratings.length > 0
    $('anchor-note').hidden = !ratings.some((r) => r.anchor)
    const isBot = (r) => !r.build
    $('bot-note').hidden = !ratings.some(isBot)
    $('ranking')
        .querySelector('tbody')
        .replaceChildren(
            ...ratings.map((r, i) => {
                const bot = isBot(r)
                const team = el('td', { class: 'team', title: r.team }, r.team)
                const v = version(r)
                if (v) team.append(el('span', { class: 'badge', title: `Build ${r.build}` }, v))
                if (bot) team.append(el('span', { class: 'badge', title: 'Adversaire de référence' }, 'bot'))
                if (r.anchor) team.append(el('span', { class: 'badge anchor' }, 'ancre'))
                if (r.provisional) team.append(el('span', { class: 'badge' }, 'provisoire'))
                return el(
                    'tr',
                    bot ? { class: 'bot' } : {},
                    el('td', { class: 'num rank' }, String(i + 1)),
                    team,
                    el('td', { class: 'num elo' }, String(Math.round(r.rating))),
                    el('td', { class: 'num record' }, `${r.wins}–${r.losses}`),
                    el(
                        'td',
                        {
                            class: 'num pct',
                            title: `${r.build_wins} victoires sur ${r.build_games} parties${v ? ` de ${v}` : ''}`,
                        },
                        `${Math.round((100 * r.build_wins) / r.build_games)} %`
                    ),
                    paceCell(r.pace),
                    el(
                        'td',
                        { class: 'num last-col', title: r.last ? fullFormat.format(new Date(r.last)) : '' },
                        r.last ? ago(r.last) : ''
                    )
                )
            })
        )
}

function renderChart(ratings, rows, key, team) {
    const r = team && ratings.find((r) => r.team === team)
    $('elo-chart').hidden = !r
    const history = r
        ? rows.map((g) => {
            const side = g.red.team === r.team ? g.red : g.black
            const [rating, delta] = side[key]
            return { id: g.id, ts: g.ts, rating, delta, game: g, build: side.build, version: version(side) }
        })
        : []
    for (const k of versionStarts(history)) history[k].newVersion = true
    const w = document.querySelector('#elo-window input:checked')?.value
    const { start, points: recent } = r ? since(history, Date.now() - (WINDOWS[w] ?? Infinity)) : { points: [] }
    $('elo-plot').hidden = !recent.length
    if (!recent.length) {
        chart.points = []
        $('elo-versions').replaceChildren()
        if (r)
            $('elo-caption').replaceChildren(
                el('strong', {}, `${Math.round(start)} Elo`),
                ' aucune partie dans cette fenêtre'
            )
        return
    }
    const before = recent.length < history.length
    const points = [{ rating: start, before }, ...recent]
    const values = points.map((p) => p.rating)
    const min = Math.min(...values)
    const max = Math.max(...values)
    chart = { team: r.team, points, min, max, index: 0 }

    const svg = $('elo-line')
    const bottom = max - min || 1
    const line = values.map((v, i) => `${i},${max - v}`).join(' ')
    const area = document.createElementNS(svg.namespaceURI, 'polygon')
    area.setAttribute('points', `0,${bottom} ${line} ${values.length - 1},${bottom}`)
    const stroke = document.createElementNS(svg.namespaceURI, 'polyline')
    stroke.setAttribute('points', line)
    svg.setAttribute('viewBox', `0 0 ${values.length - 1} ${bottom}`)
    svg.replaceChildren(area, stroke)
    $('elo-versions').replaceChildren(
        ...points.flatMap((p, i) => {
            if (!p.newVersion) return []
            const left = (i - 1) / (points.length - 1)
            const marker = el('div', { class: `elo-version${left > 0.9 ? ' end' : ''}`, 'data-label': p.version })
            marker.style.left = `${100 * left}%`
            return [marker]
        })
    )
    $('elo-plot').setAttribute('aria-valuemax', String(values.length - 1))
    placeCursor(replayPoint())
}

// Point de la partie affichée dans le replay, sinon le dernier.
function replayPoint() {
    const i = chart.points.findIndex((p) => p.id && p.id === replay.game?.id)
    return i > 0 ? i : chart.points.length - 1
}

// Curseur et lecture au point i, comme un graphe de portefeuille : Elo, delta, résultat, date.
function placeCursor(i) {
    const { points, min, max, team } = chart
    if (!points.length) return
    chart.index = Math.max(0, Math.min(points.length - 1, i))
    const p = points[chart.index]
    const x = `${(100 * chart.index) / (points.length - 1)}%`
    $('elo-cursor').style.left = x
    Object.assign($('elo-dot').style, { left: x, top: `${(100 * (max - p.rating)) / (max - min || 1)}%` })

    const parts = [el('strong', {}, `${Math.round(p.rating)} Elo`)]
    let text = `${Math.round(p.rating)} Elo`
    if (p.game) {
        const color = p.game.red.team === team ? 'red' : 'black'
        const opponent = p.game[color === 'red' ? 'black' : 'red'].team
        const d = roundDelta(p.delta)
        const release = p.newVersion ? `, nouvelle version ${p.version}` : ''
        const context = ` ${p.game.winner === color ? 'victoire' : 'défaite'} contre ${opponent}, ${fullFormat.format(new Date(p.ts))}${release}`
        if (d) parts.push(' ', el('span', { class: `delta ${d > 0 ? 'up' : 'down'}` }, signed(d)))
        parts.push(el('span', {}, context))
        text += `${d ? ` (${signed(d)})` : ''},${context}`
    } else {
        const context = p.before ? ' début de la fenêtre' : ` départ de ${team}`
        parts.push(el('span', {}, context))
        text += `,${context}`
    }
    $('elo-caption').replaceChildren(...parts)
    $('elo-plot').setAttribute('aria-valuenow', String(chart.index))
    $('elo-plot').setAttribute('aria-valuetext', text)
}

function openPoint(i) {
    const g = chart.points[i]?.game
    if (g) showGame(g, true)
}

function renderGames(list, key) {
    const items = list
        .slice(-RECENT_GAMES)
        .reverse()
        .map((g) => {
            const change = (color) => roundDelta(g[color][key][1])
            const side = (color) => {
                const node = el(
                    'span',
                    { class: `side ${color}${g.winner === color ? ' winner' : ''}` },
                    el('span', {}, g[color].team)
                )
                const d = change(color)
                if (d) {
                    const title = `Elo après la partie : ${Math.round(g[color][key][0])}`
                    node.append(el('span', { class: `delta ${d > 0 ? 'up' : 'down'}`, title }, signed(d)))
                }
                return node
            }
            const who = (color, name) => `${g[color].team} (${name}${change(color) ? `, ${signed(change(color))}` : ''})`
            const button = el(
                'button',
                {
                    type: 'button',
                    class: 'game',
                    'data-id': g.id,
                    'aria-label': `${who('red', 'rouge')} contre ${who('black', 'noir')}, ${g[g.winner].team} gagne`,
                },
                el('time', { datetime: g.ts, title: fullFormat.format(new Date(g.ts)) }, shortWhen(g.ts)),
                side('red'),
                side('black'),
                el(
                    'span',
                    { class: 'meta' },
                    `${REASONS[g.reason] ?? g.reason}, ${g.moves} coups, ${Math.round(g.duration_ms / 1000)} s`
                )
            )
            if (replay.game?.id === g.id) button.setAttribute('aria-current', 'true')
            button.addEventListener('click', () => showGame(g, true))
            return el('li', {}, button)
        })
    $('games').replaceChildren(
        ...(items.length ? items : [el('li', { class: 'empty' }, 'Les parties jouées apparaîtront ici.')])
    )
}

async function loadGame(row) {
    const [start, length] = row.at
    const res = await fetch(`${DATA}/games.jsonl`, { headers: { Range: `bytes=${start}-${start + length - 1}` } })
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    const bytes = new Uint8Array(await res.arrayBuffer())
    // 200 : serveur qui ignore Range, fichier entier.
    const line = res.status === 200 ? bytes.subarray(start, start + length) : bytes
    const g = JSON.parse(new TextDecoder().decode(line))
    if (g.id !== row.id) throw new Error('partie introuvable')
    return g
}

// Un échec (copie en cache de games.jsonl plus vieille que le résumé) est réessayé au rafraîchissement suivant.
async function showGame(row, pinned = replay.pinned) {
    const seq = ++replaySeq
    let g
    try {
        g = await loadGame(row)
    } catch (e) {
        if (seq === replaySeq) $('replay-info').textContent = `Partie pas encore disponible (${e.message}).`
        return
    }
    if (seq !== replaySeq) return
    stopPlayback()
    const steps = positions(g)
    // moveMs[k] : temps du coup moves[k], vide si absent ou incohérent.
    const moveMs = g.move_ms?.length === g.moves.length ? g.move_ms : []
    replay = { game: g, steps, moveMs, index: steps.length - 1, timer: null, pinned }
    setParam('game', pinned ? g.id : null)
    $('step').max = String(steps.length - 1)

    const loser = g.winner === 'red' ? 'black' : 'red'
    const ending = ENDINGS[g.reason]?.(g[loser].team) ?? g.reason
    const name = (color) => [g[color].team, version(g[color])].filter(Boolean).join(' ')
    $('replay-info').replaceChildren(
        el('strong', {}, name('red')),
        ' (rouge) contre ',
        el('strong', {}, name('black')),
        ` (noir) : ${g[g.winner].team} gagne, ${ending}.`
    )
    for (const button of $('games').querySelectorAll('.game')) {
        if (button.dataset.id === g.id) button.setAttribute('aria-current', 'true')
        else button.removeAttribute('aria-current')
    }
    placeCursor(replayPoint())
    drawStep()
}

function drawStep() {
    const { steps, moveMs, index } = replay
    const step = steps[index]
    const board = $('board')
    const nodes = []
    for (let y = 0; y < SIZE; y++) {
        nodes.push(el('div', { class: 'label' }, String(SIZE - y)))
        for (let x = 0; x < SIZE; x++) {
            const i = y * SIZE + x
            const v = step.cells[i]
            const cell = el('div', { class: 'cell' })
            if (v === DESTROYED) cell.classList.add('hole')
            if (step.move) {
                if (i === step.move.from) cell.classList.add('from')
                if (i === step.move.to) cell.classList.add('to')
                if (i === step.move.destroyed) cell.classList.add('destroyed-now')
            }
            if (v === RED) cell.append(el('div', { class: 'pawn red' }))
            if (v === BLACK) cell.append(el('div', { class: 'pawn black' }))
            nodes.push(cell)
        }
    }
    nodes.push(el('div'))
    for (let x = 0; x < SIZE; x++) nodes.push(el('div', { class: 'label' }, String.fromCharCode(65 + x)))
    board.replaceChildren(...nodes)

    // Les rouges jouent les coups impairs : 1, 3, 5...
    board.dataset.mover = index % 2 === 1 ? 'red' : 'black'
    const total = steps.length - 1
    $('step').value = String(index)
    if (step.move) {
        const { from, to, destroyed } = step.move
        // Les coups des clients sont les derniers pas, après l'ouverture.
        const ms = moveMs[index - (steps.length - moveMs.length)]
        const detail = step.opening ? ', ouverture' : ms !== undefined ? `, ${ms} ms` : ''
        $('move-count').textContent = `Coup ${index} sur ${total}${detail}`
        $('move-text').replaceChildren(
            `${squareName(from)} → ${squareName(to)} `,
            el('span', { class: 'destroyed' }, `détruit ${squareName(destroyed)}`)
        )
        board.setAttribute(
            'aria-label',
            `Coup ${index} sur ${total} : ${squareName(from)} vers ${squareName(to)}, ${squareName(destroyed)} détruite`
        )
    } else {
        $('move-count').textContent = `Position initiale, ${total} coups`
        $('move-text').replaceChildren()
        board.setAttribute('aria-label', 'Plateau 12 × 12, position initiale')
    }
}

function goTo(i) {
    if (!replay.game) return
    replay.index = Math.max(0, Math.min(replay.steps.length - 1, i))
    drawStep()
}

function stopPlayback() {
    clearInterval(replay.timer)
    replay.timer = null
    $('play').classList.remove('playing')
    $('play').setAttribute('aria-label', 'Lecture')
}

function togglePlayback() {
    if (!replay.game) return
    if (replay.timer) return stopPlayback()
    replay.pinned = true
    if (replay.index === replay.steps.length - 1) goTo(0)
    $('play').classList.add('playing')
    $('play').setAttribute('aria-label', 'Pause')
    replay.timer = setInterval(() => {
        if (replay.index >= replay.steps.length - 1) return stopPlayback()
        goTo(replay.index + 1)
    }, PLAY_MS)
}

// Parcourir une partie la fige : le replay ne saute plus vers la dernière jouée.
function browse(i) {
    replay.pinned = true
    setParam('game', replay.game?.id)
    goTo(i)
}

async function share() {
    if (!replay.game) return
    replay.pinned = true
    setParam('game', replay.game.id)
    try {
        await navigator.clipboard.writeText(location.href)
        $('share').textContent = 'Lien copié'
    } catch {
        $('share').textContent = "Copiez l'adresse de la page"
    }
    setTimeout(() => ($('share').textContent = 'Copier le lien'), 2000)
}

function reselect() {
    replay.pinned = false
    render()
    follow().then(render, render)
}

const picker = createPicker({
    ratings: () => (summary ? summary.scopes[$('session').value].ratings : []),
    onChange: (teams) => {
        chosen = teams
        setParam('teams', teams.join(','))
        reselect()
    },
})
$('session').addEventListener('change', () => {
    setParam('session', $('session').value, true)
    reselect()
})
for (const input of document.querySelectorAll('#match input')) input.checked = input.value === (params.get('match') ?? GLOBAL)
$('match').addEventListener('change', (e) => {
    setParam('match', e.target.value)
    reselect()
})
$('share').addEventListener('click', share)
for (const input of document.querySelectorAll('#elo-window input'))
    input.checked = input.value === (WINDOW in WINDOWS ? WINDOW : GLOBAL)
$('elo-window').addEventListener('change', (e) => {
    setParam('window', e.target.value)
    render()
})

const plot = $('elo-plot')
const pointAt = (e) => {
    const box = plot.getBoundingClientRect()
    return Math.round(((e.clientX - box.left) / box.width) * (chart.points.length - 1))
}
for (const type of ['pointermove', 'pointerdown']) plot.addEventListener(type, (e) => placeCursor(pointAt(e)))
plot.addEventListener('pointerleave', () => placeCursor(replayPoint()))
// Le point affiché, pas la position du clic : `click` arrondit clientX et peut tomber sur le voisin.
plot.addEventListener('click', () => openPoint(chart.index))
plot.addEventListener('keydown', (e) => {
    const moves = { ArrowLeft: chart.index - 1, ArrowRight: chart.index + 1, Home: 0, End: chart.points.length - 1 }
    if (e.key in moves) placeCursor(moves[e.key])
    else if (e.key === 'Enter' || e.key === ' ') openPoint(chart.index)
    else return
    e.preventDefault()
})
$('first').addEventListener('click', () => browse(0))
$('prev').addEventListener('click', () => browse(replay.index - 1))
$('next').addEventListener('click', () => browse(replay.index + 1))
$('last').addEventListener('click', () => browse(replay.steps.length - 1))
$('play').addEventListener('click', togglePlayback)
$('step').addEventListener('input', (e) => browse(+e.target.value))
document.addEventListener('keydown', (e) => {
    if (e.target.closest('input, select, textarea, [contenteditable], [role="slider"]')) return
    if (e.key === 'ArrowLeft') browse(replay.index - 1)
    if (e.key === 'ArrowRight') browse(replay.index + 1)
})

// Le formulaire n'apparaît que si le service de soumission répond (absent en hébergement statique).
const API = `${SERVER}/api`

async function setupSubmit() {
    try {
        const res = await fetch(`${API}/health`, { cache: 'no-store' })
        if (!res.ok) return
    } catch {
        return
    }
    $('submit-panel').hidden = false
    const status = $('submit-status')
    $('submit-file').addEventListener('change', (e) => {
        $('submit-file-name').textContent = e.target.files[0]?.name ?? 'Choisir le jar'
    })
    $('submit-form').addEventListener('submit', async (e) => {
        e.preventDefault()
        const file = $('submit-file').files[0]
        $('submit-button').disabled = true
        status.dataset.state = 'pending'
        status.textContent = "Parties de test en cours : une à deux minutes, plus si d'autres soumissions attendent…"
        try {
            const res = await fetch(`${API}/submit`, {
                method: 'PUT',
                headers: { Authorization: `Bearer ${$('submit-token').value.trim()}` },
                body: file,
            })
            status.dataset.state = res.ok ? 'ok' : 'error'
            status.textContent = (await res.text()).trim()
        } catch (err) {
            status.dataset.state = 'error'
            status.textContent = `Envoi impossible : ${err.message}. Vérifiez la connexion, puis réessayez.`
        } finally {
            $('submit-button').disabled = false
        }
    })
}

refresh()
setInterval(refresh, REFRESH_MS)
setupSubmit()
linkPages()
