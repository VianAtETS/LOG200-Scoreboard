// Page Tournoi : tableau à double élimination d'un tournoi hebdomadaire, lu dans data/tournaments/, ou
// aperçu du prochain (data/tournament-preview.json). ?tournament= suit le tableau affiché.

import { $, DATA, el, fullFormat, linkPages, liveLink, pageUrl, params, pulse, REFRESH_MS, setParam, showVersion, timeFormat } from './common.js'
import { correction, tournamentDuration } from './estimate.js'
import { fetchModel, percent, winProbability } from './predict.js'

const SIDES = [
    ['winners', 'Tableau principal'],
    ['losers', 'Repêchage'],
]
const STATUS = { running: 'en cours', suspended: 'suspendu' }
const PREVIEW = 'apercu'
const CALIBRATION = 5
const dateFormat = new Intl.DateTimeFormat('fr-CA', { weekday: 'long', day: 'numeric', month: 'long' })
const shortDate = new Intl.DateTimeFormat('fr-CA', { day: 'numeric', month: 'short', year: 'numeric' })

let index = null
let view = null
let viewText = null
let previewText = null
let builds = new Map()
const finished = new Map()
let factor = 1
let model = null
let running = []

async function json(url) {
    const res = await fetch(url, { cache: 'no-store' })
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    return res.json()
}

async function refresh() {
    try {
        index = await json(`${DATA}/tournaments/index.json`).catch(() => ({ next: null, tournaments: [] }))
        builds = new Map(Object.entries((await json(`${DATA}/summary.json`).catch(() => ({}))).builds ?? {}))
        const recent = index.tournaments.filter((t) => t.winner).slice(0, CALIBRATION)
        for (const t of recent.filter((t) => !finished.has(t.id))) {
            finished.set(t.id, await json(`${DATA}/tournaments/${encodeURIComponent(t.id)}.json`).catch(() => null))
        }
        factor = correction(recent.map((t) => finished.get(t.id)).filter(Boolean))
        model = await fetchModel(DATA)
        running = await json(`${DATA}/live.json`).then((file) => file.games).catch(() => [])
        previewText = await fetch(`${DATA}/tournament-preview.json`, { cache: 'no-store' })
            .then((res) => (res.ok ? res.text() : null))
            .catch(() => null)
        renderPicker()
        const id = $('tournament').value
        let changed = false
        if (id) {
            const text = id === PREVIEW ? previewText : await fetchView(id)
            changed = viewText !== null && text !== viewText
            if (text !== viewText) view = JSON.parse(text)
            viewText = text
        } else {
            view = null
        }
        render()
        if (changed) pulse()
        $('live').className = 'live on'
    } catch (e) {
        $('live').className = 'live error'
        $('status').textContent = `Tournoi indisponible : ${e.message}`
    }
}

async function fetchView(id) {
    const res = await fetch(`${DATA}/tournaments/${encodeURIComponent(id)}.json`, { cache: 'no-store' })
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    return res.text()
}

// Par défaut : le tournoi en cours, sinon l'aperçu du prochain, sinon le dernier.
function renderPicker() {
    const select = $('tournament')
    const wanted = select.value || params.get('tournament')
    const options = index.tournaments.map((t) => el('option', { value: t.id }, shortDate.format(new Date(t.ts))))
    if (previewText) options.unshift(el('option', { value: PREVIEW }, 'Prochain (aperçu)'))
    select.replaceChildren(...options)
    const ids = options.map((o) => o.value)
    const running = index.tournaments.find((t) => !t.winner)?.id
    select.value = [wanted, running, previewText && PREVIEW, ids[0]].find((id) => id && ids.includes(id)) ?? ''
    select.disabled = ids.length < 2
}

function render() {
    const next = index.next ? new Date(index.next) : null
    $('next').hidden = !next
    const preview = previewText && JSON.parse(previewText)
    const length = preview ? predicted(preview) : null
    if (next) {
        $('next').textContent = `Prochain tournoi : ${dateFormat.format(next)}, ${timeFormat.format(next)}${length ? `, durée prévue ≈ ${span(length)}` : ''}. Les jars acceptés avant le tirage jouent.`
    }
    $('tournament-empty').hidden = Boolean(view)
    $('status').textContent = `À jour à ${timeFormat.format(new Date())}`
    if (!view) {
        $('tournament-meta').textContent = ''
        $('places').hidden = true
        $('bracket').replaceChildren()
        return
    }
    const pace = `${view.time_ms < 1000 ? `${view.time_ms} ms` : `${view.time_ms / 1000} s`} par coup${timing()}`
    if (view.id === PREVIEW) {
        $('tournament-title').textContent = 'Aperçu du prochain tournoi'
        $('tournament-meta').textContent =
            `${view.entrants.length} équipes · ${pace} · le tirage si le tournoi partait maintenant, selon l'Elo vérifié de la session, recalculé après chaque partie`
    } else {
        $('tournament-title').textContent = `Tournoi du ${dateFormat.format(new Date(view.ts))}`
        const winner = view.winner ? ` · vainqueur : ${view.winner}` : ''
        $('tournament-meta').textContent =
            `${view.entrants.length} équipes · ${pace} · tiré le ${fullFormat.format(new Date(view.ts))}${winner}`
    }
    renderPlaces()
    renderBracket()
}

function predicted(v, fromStart = true) {
    const ms = tournamentDuration(v, fromStart)
    return ms === null ? null : ms * factor
}

function span(ms) {
    const minutes = Math.round(ms / 60_000)
    const h = Math.floor(minutes / 60)
    return h ? `${h} h ${String(minutes % 60).padStart(2, '0')}` : `${minutes} min`
}

function timing() {
    if (view.winner) {
        if (!view.ended) return ''
        const raw = tournamentDuration(view, true)
        return ` · a duré ${span(Date.parse(view.ended) - Date.parse(view.ts))}${raw === null ? '' : ` (prévu au tirage ≈ ${span(raw)})`}`
    }
    const planned = predicted(view)
    if (planned === null) return ''
    if (view.id === PREVIEW) return ` · durée prévue ≈ ${span(planned)}`
    return ` · fin prévue vers ${timeFormat.format(new Date(Date.now() + predicted(view, false)))}`
}

function describe(team) {
    const entrant = view.entrants.find((e) => e.team === team)
    const n = (builds.get(team)?.indexOf(entrant?.build) ?? -1) + 1
    return [team, n ? `v${n}` : '', entrant ? `· ${Math.round(entrant.rating)} Elo` : ''].filter(Boolean).join(' ')
}

function renderPlaces() {
    const ranked = view.places ? view.entrants.map((e, i) => ({ team: e.team, place: view.places[i] })) : []
    ranked.sort((a, b) => a.place - b.place || a.team.localeCompare(b.team))
    $('places').hidden = !ranked.length
    $('places').replaceChildren(
        ...ranked.map(({ team, place }) =>
            el(
                'li',
                { class: place === 1 ? 'first' : '' },
                el('span', { class: 'place' }, place === 1 ? '1er' : `${place}e`),
                el('span', {}, team)
            )
        )
    )
}

function roundTitle(side, round, last) {
    if (side === 'final') return 'Grande finale'
    if (round === last) return side === 'winners' ? 'Finale du principal' : 'Finale du repêchage'
    return side === 'winners' ? `Tour ${round}` : `Repêchage ${round}`
}

function renderBracket() {
    const seeds = new Map(view.entrants.map((e, i) => [e.team, i + 1]))
    const byId = new Map(view.matches.map((m) => [m.id, m]))
    const sections = SIDES.map(([side, title]) => {
        const matches = view.matches.filter((m) => m.side === side)
        const last = Math.max(...matches.map((m) => m.round))
        const rounds = []
        for (let r = 1; r <= last; r++) {
            rounds.push([roundTitle(side, r, last), matches.filter((m) => m.round === r)])
        }
        if (side === 'winners') rounds.push([roundTitle('final'), view.matches.filter((m) => m.side === 'final')])
        const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg')
        svg.classList.add('links')
        svg.setAttribute('aria-hidden', 'true')
        return el(
            'section',
            { class: 'bracket-section', 'aria-label': title },
            el('h3', { class: 'band' }, title),
            el(
                'div',
                { class: 'rounds' },
                svg,
                ...rounds.map(([name, list]) =>
                    el('div', { class: 'round' }, el('p', { class: 'round-title' }, name), el('div', { class: 'round-matches' }, ...list.map((m) => card(m, seeds, byId))))
                )
            )
        )
    })
    $('bracket').replaceChildren(...sections)
    requestAnimationFrame(drawLinks)
}

function card(m, seeds, byId) {
    const bye = m.status === 'done' && !m.games.length
    const wrap = el('div', {
        class: 'match-wrap',
        'data-id': m.id,
        'data-from': m.from.flatMap((f) => (f.winner ? [f.winner] : [])).join(','),
    })
    if (bye && !m.teams[0] && !m.teams[1]) return wrap

    const awaiting = (from) => {
        const source = byId.get(from.winner ?? from.loser)
        if (bye || (source?.status === 'done' && !source.games.length && (from.loser || !source.winner))) return 'exempt'
        return from.winner ? `vainqueur ${from.winner}` : from.loser ? `perdant ${from.loser}` : 'à venir'
    }
    const scored = m.games.length > 0 || m.head_start.some(Boolean)
    const open = m.teams[0] && m.teams[1] && m.status !== 'done' && !scored
    const slot = (i) => {
        const team = m.teams[i]
        const state = !team ? ' tbd' : bye ? '' : m.winner === team ? ' win' : m.winner ? ' lose' : ''
        const chance = open ? winProbability(model, team, m.teams[1 - i]) : null
        const score = el('span', { class: 'score' }, team && scored ? String(m.score[i]) : '')
        if (chance !== null) {
            score.classList.add('prob')
            score.textContent = `${percent(chance)}%`
            score.title = `${percent(chance)} % de chances de gagner une manche, selon le modèle (couleurs moyennées)`
        }
        return el(
            'div',
            { class: `slot${state}` },
            el('span', { class: 'seed' }, team ? String(seeds.get(team)) : ''),
            el('span', { class: 'name', title: team ? describe(team) : '' }, team ?? awaiting(m.from[i])),
            score
        )
    }
    const label = [m.id, m.best_of === 5 ? 'meilleur de 5, le vainqueur du principal part à 1–0' : `meilleur de ${m.best_of}`]
    const head = el('p', { class: 'match-id' }, m.id)
    if (STATUS[m.status]) head.append(el('span', { class: `match-state ${m.status}` }, STATUS[m.status]))
    const live = m.status === 'running' && running.find((g) => g.id && g.tournament?.id === view.id && g.tournament.match === m.id)
    if (live) head.append(liveLink(live.id, 'Suivre la manche en direct'))
    if (m.side === 'final') head.append(el('span', { class: 'match-note' }, 'meilleur de 5, avance de 1–0'))
    wrap.append(
        head,
        el(
            'div',
            { class: `match ${m.status}${bye ? ' bye' : ''}`, 'aria-label': `${label.join(', ')} : ${m.teams.map((t) => t ?? 'à venir').join(' contre ')}` },
            slot(0),
            slot(1)
        )
    )
    if (m.games.length) {
        wrap.append(
            el(
                'p',
                { class: 'match-games' },
                'Manches ',
                ...m.games.map((id, k) => el('a', { href: pageUrl('./', { game: id }), title: 'Rejouer cette manche' }, String(k + 1)))
            )
        )
    }
    return wrap
}

// Liaison coudée entre la carte d'un match et celle du match où va son vainqueur.
function drawLinks() {
    for (const rounds of document.querySelectorAll('.rounds')) {
        const svg = rounds.querySelector('svg.links')
        const box = rounds.getBoundingClientRect()
        const paths = []
        for (const target of rounds.querySelectorAll('.match-wrap[data-from]')) {
            const to = target.querySelector('.match')?.getBoundingClientRect()
            if (!to?.width) continue
            for (const id of target.dataset.from.split(',').filter(Boolean)) {
                const from = rounds.querySelector(`[data-id="${id}"] .match`)?.getBoundingClientRect()
                if (!from?.width) continue
                const [ax, ay] = [from.right - box.left, from.top + from.height / 2 - box.top]
                const [bx, by] = [to.left - box.left, to.top + to.height / 2 - box.top]
                const mx = (ax + bx) / 2
                const path = document.createElementNS(svg.namespaceURI, 'path')
                path.setAttribute('d', `M${ax},${ay} H${mx} V${by} H${bx}`)
                paths.push(path)
            }
        }
        svg.setAttribute('width', String(rounds.scrollWidth))
        svg.setAttribute('height', String(rounds.scrollHeight))
        svg.replaceChildren(...paths)
    }
}

$('tournament').addEventListener('change', () => {
    setParam('tournament', $('tournament').value)
    viewText = null
    refresh()
})
addEventListener('resize', () => requestAnimationFrame(drawLinks))

refresh()
setInterval(refresh, REFRESH_MS)
linkPages()
showVersion()
