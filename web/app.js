// Page du ladder : classement et dernières parties de summary.json, parties des équipes choisies
// dans teams/, replay lu partie par partie sur /api/games/<id>.
// ?session=, ?teams=, ?match=, ?window=, ?verified=, ?pace=, ?record= et ?game= suivent les choix de la page : l'adresse
// se partage telle quelle. ?team= des anciens liens vaut ?teams= avec cette seule équipe.

import {
    $,
    DATA,
    dayFormat,
    el,
    fillSelect,
    fullFormat,
    GLOBAL,
    initVerified,
    linkPages,
    liveLink,
    pageUrl,
    params,
    pulse,
    REFRESH_MS,
    roundDelta,
    scopesOf,
    SERVER,
    setParam,
    showVersion,
    signed,
    tail,
    timeFormat,
    WINDOWS,
} from './common.js'
import { BETWEEN, gamesOf, isRemote, ratingOf, record, since, versionStarts } from './elo.js'
import { createPicker } from './picker.js'
import { createArchiveFilter } from './archive-filter.js'
import { withArchivedRows } from './archives.js'
import { fetchModel, percent, winProbability } from './predict.js'
import { renderBoard } from './board.js'
import { connectLive, liveUrl, pickLive } from './live.js'
import { positions, squareName } from './replay.js'
import { createQueueView } from './queue.js'

const RECENT_GAMES = 50
const PLAY_MS = 500
// p99 au-delà de cette part du temps par coup : l'équipe frôle le timeout.
const PACE_WARN = 0.8
// Mesure de la colonne de temps, dans l'ordre du clic : libellé et titre.
const PACE_STATS = {
    mean: ['moy.', 'Temps moyen par coup'],
    p50: ['méd.', 'Temps médian par coup'],
    p95: ['p95', '95e centile du temps par coup'],
    p99: ['p99', '99e centile du temps par coup'],
    max: ['max', 'Temps maximal par coup'],
}
let paceStat = params.get('pace') in PACE_STATS ? params.get('pace') : 'p95'
const RECORD_STATS = {
    all: ['V–D', 'Victoires et défaites, toutes versions'],
    build: ['V–D v.', 'Victoires et défaites, version actuelle'],
}
let recordStat = params.get('record') in RECORD_STATS ? params.get('record') : 'all'
const ELO_STATS = {
    rating: ['Elo', 'Elo actuel'],
    peak: ['Peak', 'Elo maximal atteint'],
}
let eloStat = params.get('elo') in ELO_STATS ? params.get('elo') : 'rating'
const REMOTE_TITLE =
    "Joue depuis la machine de l'équipe : code non vérifié, sans limite de coeurs ni de mémoire, latence réseau comptée"
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
// Parties en cours, d'après live.json.
let running = []
// Modèle de probabilité de model.json, `null` s'il n'est pas publié.
let model = null
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
        archives.update(summary)
        await archives.load()
        changed = (await follow()) || changed
        running = await fetchRunning().catch(() => [])
        if ($('queue').open) showQueue()
        model = await fetchModel(DATA)
        if (first && GAME && running.some((g) => g.id === GAME)) ensureLive()
        else if (first && GAME) openLinked().catch(() => (replay.pinned = false))
        const teams = selectedTeams()
        if (running.some((g) => g.id && (teams.includes(g.red) || teams.includes(g.black)))) ensureLive()
        render()
        if (changed && !first) pulse()
        $('live').className = 'live on'
    } catch (e) {
        $('live').className = 'live error'
        $('status').textContent = `Données indisponibles : ${e.message}`
    }
}

// Absent avec un ladder qui ne publie pas encore live.json : rien n'est affiché.
async function fetchRunning() {
    const res = await fetch(`${DATA}/live.json`, { cache: 'no-store' })
    return res.ok ? (await res.json()).games : []
}

const queueView = createQueueView({ tree: $('queue-tree'), lede: $('queue-lede') })
$('queue').addEventListener('close', () => queueView.reset())

// Absent avec un ladder qui ne publie pas encore queue.json : `null`, et le panneau le dit.
async function showQueue() {
    const res = await fetch(`${DATA}/queue.json`, { cache: 'no-store' }).catch(() => null)
    const file = res?.ok ? await res.json().catch(() => null) : null
    queueView.show(file?.teams ?? null, running, file?.next ?? [])
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

const outside = (g, session) => session === GLOBAL && Boolean(g.tournament)

const matchMode = () => document.querySelector('#match input:checked')?.value ?? GLOBAL

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

let liveGames = null

function ensureLive() {
    if (liveGames) return
    liveGames = new Map()
    connectLive(liveUrl(SERVER, location.href), (games, state) => {
        liveGames = games
        if (!followLive() && GAME && !replay.game && state === 'unavailable')
            $('replay-info').textContent = 'Direct indisponible sur ce serveur.'
    })
}

function followLive() {
    if (!liveGames) return false
    const teams = selectedTeams()
    const shown = replay.game?.live ? liveGames.get(replay.game.id) : undefined
    const g = replay.pinned
        ? liveGames.get(replay.game?.id ?? GAME)
        : teams.length
            ? (pickLive(liveGames, teams, matchMode() === BETWEEN) ?? shown)
            : undefined
    if (g) showLive(g)
    return Boolean(g)
}

function showLive(g) {
    const following = !replay.game || replay.index === replay.steps.length - 1
    const steps = positions(g)
    replay = {
        ...replay,
        game: { id: g.id, red: { team: g.red }, black: { team: g.black }, live: true },
        steps,
        moveMs: g.move_ms,
        index: following ? steps.length - 1 : replay.index,
    }
    $('step').max = String(steps.length - 1)
    const { winner, reason, cancelled, interrupted } = g.ended ?? {}
    const loser = winner === 'red' ? g.black : g.red
    const state = !g.ended
        ? 'en direct'
        : cancelled
            ? 'partie annulée'
            : interrupted
                ? 'direct interrompu'
                : `${g[winner]} gagne, ${ENDINGS[reason]?.(loser) ?? reason}`
    $('replay-info').replaceChildren(el('strong', {}, g.red), ' (rouge) contre ', el('strong', {}, g.black), ` (noir) : ${state}.`)
    drawStep()
}

function markOverflow(node) {
    node.classList.toggle('less', node.scrollTop > 2)
    node.classList.toggle('more', node.scrollTop + node.clientHeight < node.scrollHeight - 2)
}

const scrollAreas = [document.querySelector('.ranking-scroll'), $('games')]
for (const node of scrollAreas) node.addEventListener('scroll', () => markOverflow(node), { passive: true })
addEventListener('resize', () => scrollAreas.forEach(markOverflow))

const rankingBody = $('ranking').querySelector('tbody')
const opponentRow = (event) => {
    const team = event.target.closest?.('[data-opponent]')?.dataset.opponent
    return team && [...rankingBody.rows].find((row) => row.dataset.team === team)
}
const clearOpponent = () => rankingBody.querySelector('.opponent')?.classList.remove('opponent')
const showOpponent = (event) => {
    clearOpponent()
    opponentRow(event)?.classList.add('opponent')
}
rankingBody.addEventListener('pointerover', showOpponent)
rankingBody.addEventListener('focusin', showOpponent)
rankingBody.addEventListener('pointerout', clearOpponent)
rankingBody.addEventListener('focusout', clearOpponent)
rankingBody.addEventListener('click', (event) => {
    const row = opponentRow(event)
    if (!row) return
    row.scrollIntoView({ block: 'nearest' })
    row.classList.add('ping')
    setTimeout(() => row.classList.remove('ping'), 1200)
})

function renderSessions(first) {
    fillSelect($('session'), summary.sessions, first ? (SESSION ?? summary.sessions[0]) : $('session').value)
    if (first && params.has('team')) {
        setParam('team', null)
        setParam('teams', selectedTeams().join(','))
    }
}

function render() {
    if (!summary) return
    const session = $('session').value
    const { verified, scopes } = scopesOf(summary)
    const scope = scopes[session]
    const key = session === GLOBAL ? 'all' : 'session'
    const rating = (side) => ratingOf(side, key, verified)
    const teams = selectedTeams()
    const rows = new Map([...followed].map(([team, state]) => [team, state.rows]))
    const list = teams.length
        ? gamesOf(rows, teams, { mode: matchMode(), session }).filter((g) => (!verified || !isRemote(g)) && !outside(g, session))
        : scope.recent
    renderPaceHeader()
    renderRecordHeader()
    renderEloHeader()
    renderRanking(scope.ratings, archives.rows({ session, verified }))
    picker.render(teams)
    renderDuel(teams, scope.ratings, rows, session, verified)
    $('match').hidden = teams.length < 2
    renderChart(scope.ratings, list, rating, teams.length === 1 ? teams[0] : null)
    $('graph-link').hidden = teams.length < 2
    $('graph-link').href = pageUrl('graphe.html', { teams: teams.join(',') })
    renderGames(list, rating)

    const time = scope.recent.at(-1)?.time_ms
    $('time-ms').textContent = time ?? '?'
    const pace = time ? `, ${time} ms par coup` : ''
    const count = teams.length ? list.length : scope.games
    $('status').textContent = `${count} parties${pace}, à jour à ${timeFormat.format(new Date())}`

    scrollAreas.forEach(markOverflow)

    const latest = list.at(-1)
    const newer = latest && !(replay.game?.live && latest.id <= replay.game.id)
    if (!followLive() && newer && !replay.pinned && replay.game?.id !== latest.id) showGame(latest)
}

function duelIcon() {
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg')
    svg.setAttribute('class', 'duel-icon')
    svg.setAttribute('viewBox', '0 0 20 16')
    svg.setAttribute('aria-hidden', 'true')
    svg.innerHTML = `
        <path class="duel-sword red" d="M3 1l12 12m-2-1 3 3m-5-1 4-4" />
        <path class="duel-sword black" d="M17 1L5 13m2-1-3 3m5-1-4-4" />`
    return svg
}

function duelBadge(opponent, color, listed, chance) {
    const odds = chance === null ? '' : `, ${percent(chance)} % de chances de gagner selon le modèle`
    const title = `Partie en cours contre ${opponent}${odds}`
    const body = [duelIcon(), el('span', { class: 'duel-name' }, opponent)]
    if (chance !== null) body.push(el('span', { class: 'duel-prob' }, `${percent(chance)} %`))
    return listed
        ? el(
            'button',
            { class: `duel-badge ${color}`, type: 'button', title, 'aria-label': `${title}, voir sa ligne`, 'data-opponent': opponent },
            ...body
        )
        : el('span', { class: `duel-badge ${color}`, title }, ...body)
}

// "v3" pour le build de `player`, vide sans build.
function version(player) {
    const n = player.build ? (builds.get(player.team)?.indexOf(player.build) ?? -1) + 1 : 0
    return n ? `v${n}` : ''
}

function remoteBadge() {
    return el('span', { class: 'badge remote', title: REMOTE_TITLE }, 'distant')
}

function paceCell(pace) {
    // Sans `mean` : summary.json d'un ladder plus ancien.
    if (pace?.[paceStat] === undefined) return el('td', { class: 'num pace' })
    const mean = pace.mean === undefined ? '' : `Moyenne ${pace.mean} ms, `
    const title = `${mean}médiane ${pace.p50} ms, p95 ${pace.p95} ms, p99 ${pace.p99} ms, max ${pace.max} ms, ${pace.count} coups`
    const warn = pace.p99 >= PACE_WARN * pace.limit ? ' warn' : ''
    return el('td', { class: `num pace${warn}`, title }, `${pace[paceStat]} ms`)
}

function recordCell(r, v) {
    const all = `${r.wins}–${r.losses}`
    const build = `${r.build_wins}–${r.build_games - r.build_wins}`
    const title = r.archived ? `${v}, version archivée uniquement : ${build}` : `Toutes versions ${all}, ${v || 'version actuelle'} ${build}`
    return el('td', { class: 'num record', title }, recordStat === 'build' ? build : all)
}

function renderPaceHeader() {
    const [label, title] = PACE_STATS[paceStat]
    const button = $('pace-stat')
    button.textContent = label
    button.title = `${title}, version actuelle. Cliquer pour changer de mesure.`
}

function nextPaceStat() {
    const stats = Object.keys(PACE_STATS)
    paceStat = stats[(stats.indexOf(paceStat) + 1) % stats.length]
    setParam('pace', paceStat === 'p95' ? null : paceStat)
    render()
}

function renderRecordHeader() {
    const [label, title] = RECORD_STATS[recordStat]
    const button = $('record-stat')
    button.textContent = label
    button.title = `${title}. Cliquer pour changer.`
}

function renderEloHeader() {
    const [label, title] = ELO_STATS[eloStat]
    const button = $('elo-stat')
    button.textContent = label
    button.title = `${title}. Cliquer pour changer.`
}

function nextEloStat() {
    eloStat = eloStat === 'rating' ? 'peak' : 'rating'
    setParam('elo', eloStat === 'rating' ? null : eloStat)
    render()
}

function nextRecordStat() {
    recordStat = recordStat === 'all' ? 'build' : 'all'
    setParam('record', recordStat === 'all' ? null : recordStat)
    render()
}

// Avec deux équipes choisies : bilan de la version actuelle de chacune contre l'autre.
function renderDuel(teams, ratings, rows, session, verified) {
    $('duel').hidden = teams.length !== 2
    if (teams.length !== 2) return
    const games = gamesOf(rows, teams, { mode: BETWEEN, session }).filter((g) => (!verified || !isRemote(g)) && !outside(g, session))
    const line = (team, opponent) => {
        const build = ratings.find((r) => r.team === team)?.build
        const { wins, games: n } = record(games, team, opponent, build)
        const name = [team, version({ team, build })].filter(Boolean).join(' ')
        const score = n ? `${wins}–${n - wins} (${Math.round((100 * wins) / n)} %)` : 'aucune partie'
        return `${name} contre ${opponent} : ${score}`
    }
    $('duel').textContent = `${line(teams[0], teams[1])} · ${line(teams[1], teams[0])}`
    $('duel').title = "Version actuelle de chaque équipe, contre toutes les versions de l'autre"
}

function renderRanking(ratings, archived) {
    const ranks = new Map(ratings.map((r, i) => [r.team, i + 1]))
    ratings = withArchivedRows(ratings.filter((r) => !archives.isArchived(r.team, r.build)), archived)
    $('ranking-empty').hidden = ratings.length > 0
    $('anchor-note').hidden = !ratings.some((r) => r.anchor)
    const isBot = (r) => !r.build
    const duels = new Map()
    for (const g of running) {
        if (!duels.has(g.red)) duels.set(g.red, { id: g.id, opponent: g.black, color: 'red', chance: winProbability(model, g.red, g.black, 'red') })
        if (!duels.has(g.black)) duels.set(g.black, { id: g.id, opponent: g.red, color: 'black', chance: winProbability(model, g.black, g.red, 'black') })
    }
    $('bot-note').hidden = !ratings.some(isBot)
    $('remote-note').hidden = !ratings.some((r) => r.remote)
    const listed = new Set(ratings.map((r) => r.team))
    $('ranking')
        .querySelector('tbody')
        .replaceChildren(
            ...ratings.map((r) => {
                const bot = isBot(r)
                const line = el('div', { class: 'team-line' }, el('span', { class: 'team-name' }, r.team))
                const team = el('td', { class: 'team', title: r.team }, line)
                if (r.archived) line.append(el('span', { class: 'badge archived', title: 'Version archivée : ne joue plus de nouveaux matchs' }, 'Archivé'))
                const v = version(r)
                if (v) line.append(el('span', { class: 'badge', title: `Build ${r.build}` }, v))
                if (r.remote) line.append(remoteBadge())
                if (bot) line.append(el('span', { class: 'badge', title: 'Adversaire de référence' }, 'bot'))
                if (r.anchor) line.append(el('span', { class: 'badge anchor' }, 'ancre'))
                if (r.provisional) line.append(el('span', { class: 'badge' }, 'provisoire'))
                const duel = !r.archived && duels.get(r.team)
                if (duel) line.append(duelBadge(duel.opponent, duel.color, listed.has(duel.opponent), duel.chance))
                if (duel?.id) line.append(liveLink(duel.id, 'Suivre la partie en direct'))
                return el(
                    'tr',
                    { class: r.archived ? 'archived-row' : bot ? 'bot' : '', 'data-team': r.team,
                        ...(r.archived ? { title: 'Dernier Elo de cette version, hors classement actuel' } : {}) },
                    el('td', { class: 'num rank' }, r.archived ? '—' : String(ranks.get(r.team))),
                    team,
                    el('td', { class: 'num elo' }, String(Math.round(r[eloStat] ?? r.rating))),
                    recordCell(r, v),
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

function renderChart(ratings, rows, rating, team) {
    const r = team && ratings.find((r) => r.team === team)
    $('elo-chart').hidden = !r
    const history = r
        ? rows.map((g) => {
            const side = g.red.team === r.team ? g.red : g.black
            const [after, delta] = rating(side)
            return { id: g.id, ts: g.ts, rating: after, delta, game: g, build: side.build, version: version(side) }
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

function renderGames(list, rating) {
    const items = list
        .slice(-RECENT_GAMES)
        .reverse()
        .map((g) => {
            const change = (color) => roundDelta(rating(g[color])[1])
            const side = (color) => {
                const node = el(
                    'span',
                    { class: `side ${color}${g.winner === color ? ' winner' : ''}` },
                    el('span', {}, g[color].team)
                )
                if (g[color].remote) node.append(remoteBadge())
                const d = change(color)
                if (d) {
                    const title = `Elo après la partie : ${Math.round(rating(g[color])[0])}`
                    node.append(el('span', { class: `delta ${d > 0 ? 'up' : 'down'}`, title }, signed(d)))
                }
                return node
            }
            const who = (color, name) =>
                `${g[color].team} (${name}${g[color].remote ? ', distant' : ''}${change(color) ? `, ${signed(change(color))}` : ''})`
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
    const g = row.at ? await loadFromJsonl(row.at) : await loadFromApi(row.id)
    if (g.id !== row.id) throw new Error('partie introuvable')
    return g
}

async function loadFromApi(id) {
    const res = await fetch(`${SERVER}/api/games/${encodeURIComponent(id)}`)
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    return res.json()
}

// shortcut: ladder d'avant la base (lignes avec `at`), le temps que page et ladder se déploient ; à retirer ensuite.
async function loadFromJsonl([start, length]) {
    const res = await fetch(`${DATA}/games.jsonl`, { headers: { Range: `bytes=${start}-${start + length - 1}` } })
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    const bytes = new Uint8Array(await res.arrayBuffer())
    const line = res.status === 200 ? bytes.subarray(start, start + length) : bytes
    return JSON.parse(new TextDecoder().decode(line))
}

// Un échec (API injoignable, partie pas encore en base) est réessayé au rafraîchissement suivant.
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
    const player = (color) => [el('strong', {}, name(color)), ...(g[color].remote ? [remoteBadge()] : [])]
    $('replay-info').replaceChildren(
        ...player('red'),
        ' (rouge) contre ',
        ...player('black'),
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
    renderBoard(board, step)

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

const archives = createArchiveFilter(refresh)

const picker = createPicker({
    ratings: () => (summary ? scopesOf(summary).scopes[$('session').value].ratings : []),
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
initVerified(reselect)
for (const input of document.querySelectorAll('#match input')) input.checked = input.value === (params.get('match') ?? GLOBAL)
$('match').addEventListener('change', (e) => {
    setParam('match', e.target.value)
    reselect()
})
$('share').addEventListener('click', share)
$('pace-stat').addEventListener('click', nextPaceStat)
$('record-stat').addEventListener('click', nextRecordStat)
$('elo-stat').addEventListener('click', nextEloStat)
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
    if (e.target.closest('input, select, textarea, [contenteditable], [role="slider"]') || $('queue').open) return
    if (e.key === 'ArrowLeft') browse(replay.index - 1)
    if (e.key === 'ArrowRight') browse(replay.index + 1)
})
$('queue-open').addEventListener('click', () => {
    $('queue').showModal()
    showQueue()
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
            const headers = { Authorization: `Bearer ${$('submit-token').value.trim()}` }
            // 202 : le verdict est à interroger, Cloudflare coupant une requête à 100 s.
            let res = await fetch(`${API}/submit?async`, { method: 'PUT', headers, body: file })
            if (res.status === 202) {
                const verdict = `${SERVER}${(await res.text()).trim()}`
                do {
                    await new Promise((resolve) => setTimeout(resolve, 3000))
                    res = await fetch(verdict, { headers, cache: 'no-store' })
                } while (res.status === 202)
            }
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
showVersion()
