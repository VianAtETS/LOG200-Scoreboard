// Page Direct : toutes les parties en cours, coup par coup, depuis /api/live. Un clic ouvre la partie
// dans le replay de la page Classement, qui la suit en direct.

import { renderBoard } from './board.js'
import { $, DATA, el, linkPages, pageUrl, REFRESH_MS, SERVER, showVersion } from './common.js'
import { connectLive, ENDED_KEEP_MS, forget, liveUrl, mover } from './live.js'
import { fetchModel, percent, winProbability } from './predict.js'
import { positions } from './replay.js'

const TICK_MS = 250
const STATES = {
    connecting: 'Connexion au direct…',
    unavailable: 'Direct indisponible sur ce serveur.',
    reconnecting: 'Connexion perdue, reconnexion…',
    outdated: 'Page à mettre à jour : rechargez-la.',
}

let games = new Map()
let state = 'connecting'
let model = null
const brackets = new Map()

async function refresh() {
    model = await fetchModel(DATA)
    const ids = new Set([...games.values()].flatMap((g) => (g.tournament ? [g.tournament.id] : [])))
    for (const id of ids) {
        const res = await fetch(`${DATA}/tournaments/${encodeURIComponent(id)}.json`, { cache: 'no-store' }).catch(() => null)
        if (res?.ok) brackets.set(id, await res.json())
    }
    render()
}

function context(g) {
    if (!g.tournament) return 'Ladder'
    const { id, match, game } = g.tournament
    const m = brackets.get(id)?.matches.find((x) => x.id === match)
    const score = m?.teams[0] && m.teams[1] ? ` · ${m.teams[0]} ${m.score[0]}–${m.score[1]} ${m.teams[1]}` : ''
    return `Tournoi · ${match} · manche ${game}${score}`
}

function clock(g, now) {
    const n = g.moves.length
    if (g.ended) return `${n} coups`
    const left = Math.max(0, g.time_ms - (now - g.turnAt)) / 1000
    const last = g.move_ms.at(-1)
    const thinking = `${g[mover(g)]} réfléchit : ${left.toFixed(1)} s`
    return [`Coup ${n + 1}`, thinking, ...(last === undefined ? [] : [`dernier en ${last} ms`])].join(' · ')
}

function result({ ended, red, black }) {
    if (ended.cancelled) return 'Partie annulée'
    if (ended.interrupted) return 'Direct interrompu'
    return `${ended.winner === 'red' ? red : black} gagne`
}

function card(g, now) {
    const steps = positions(g)
    const board = el('div', { class: 'board', role: 'img', 'aria-label': `${g.red} contre ${g.black}, coup ${steps.length - 1}` })
    renderBoard(board, steps.at(-1))
    board.dataset.mover = (steps.length - 1) % 2 === 1 ? 'red' : 'black'
    const chance = winProbability(model, g.red, g.black, 'red')
    const teams = el(
        'p',
        { class: 'live-teams' },
        el('strong', { class: 'red' }, g.red),
        ' contre ',
        el('strong', { class: 'black' }, g.black),
        ...(chance === null ? [] : [el('span', { class: 'duel-prob', title: 'Chances du rouge selon le modèle, avant la partie' }, ` ${percent(chance)} %`)])
    )
    const parts = [teams, el('p', { class: 'live-meta' }, context(g)), board, el('p', { class: 'live-clock', 'data-id': g.id }, clock(g, now))]
    if (g.ended) parts.push(el('p', { class: 'live-result' }, result(g)))
    return el('a', { class: `live-game${g.ended ? ' ended' : ''}`, href: pageUrl('./', { game: g.id }) }, ...parts)
}

function render() {
    const now = performance.now()
    games = forget(games, now)
    const shown = [...games.values()]
    $('grid').replaceChildren(...shown.map((g) => card(g, now)))
    $('direct-empty').hidden = shown.length > 0 || state !== 'on'
    $('status').textContent = state === 'on' ? `${shown.length} parties en cours` : STATES[state]
    $('live').className = state === 'on' ? 'live on' : state === 'connecting' ? 'live' : 'live error'
}

function tick() {
    const now = performance.now()
    for (const node of document.querySelectorAll('.live-clock')) {
        const g = games.get(node.dataset.id)
        if (g) node.textContent = clock(g, now)
    }
    if ([...games.values()].some((g) => g.ended && now - g.endedAt >= ENDED_KEEP_MS)) render()
}

connectLive(liveUrl(SERVER, location.href), (next, nextState) => {
    const fresh = [...next.keys()].some((id) => !games.has(id))
    games = next
    state = nextState
    if (fresh) refresh()
    else render()
})
render()
setInterval(refresh, REFRESH_MS)
setInterval(tick, TICK_MS)
linkPages()
showVersion()
