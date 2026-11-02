// Serveur : la balise meta scoreboard-server (posée par le workflow GitHub Pages), sinon le site qui
// sert la page. Données : ?data=URL du dossier, sinon <serveur>/data.

import { parseJsonl } from './elo.js'
import { releaseState } from './version.js'

export const params = new URLSearchParams(location.search)
export const SERVER = document.querySelector('meta[name="scoreboard-server"]')?.content || '..'
export const DATA = params.get('data') ?? `${SERVER}/data`
export const REFRESH_MS = 15000
export const GLOBAL = ''
// Fenêtres des graphes en ms ; une valeur absente vaut "Tout".
export const WINDOWS = { '1h': 36e5, '24h': 864e5, '7j': 6048e5 }

export const $ = (id) => document.getElementById(id)

export function el(tag, attrs = {}, ...children) {
    const node = document.createElement(tag)
    for (const [k, v] of Object.entries(attrs)) {
        if (k === 'class') node.className = v
        else node.setAttribute(k, v)
    }
    node.append(...children)
    return node
}

export const timeFormat = new Intl.DateTimeFormat('fr-CA', { timeStyle: 'short' })
export const dayFormat = new Intl.DateTimeFormat('fr-CA', { day: 'numeric', month: 'short' })
export const fullFormat = new Intl.DateTimeFormat('fr-CA', { dateStyle: 'long', timeStyle: 'short' })

export const signed = (n) => `${n > 0 ? '+' : '−'}${Math.abs(n).toLocaleString('fr-CA')}`
export const roundDelta = (d) => Math.round(d) || Math.round(d * 10) / 10

// JSONL en ajout seul : lu en entier, puis seulement la suite (Range). Vrai si des lignes sont arrivées.
export async function tail(url, state) {
    const headers = state.offset ? { Range: `bytes=${state.offset}-` } : {}
    const res = await fetch(url, { cache: 'no-store', headers })
    if (res.status === 416) {
        // Rien après offset, sauf si le fichier a rétréci (réécrit) : on relit tout.
        if (Number(res.headers.get('Content-Range')?.split('/')[1]) < state.offset) {
            state.offset = 0
            return tail(url, state)
        }
        return false
    }
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    const bytes = new Uint8Array(await res.arrayBuffer())
    // Une dernière ligne en cours d'écriture est relue au tour suivant.
    const end = bytes.lastIndexOf(10) + 1
    // 200 : fichier entier (première lecture, ou serveur qui ignore Range).
    const whole = res.status === 200
    if (whole ? end === state.offset : end === 0) return false
    const added = parseJsonl(new TextDecoder().decode(bytes.subarray(0, end))).games
    state.rows = whole ? added : state.rows.concat(added)
    state.offset = (whole ? 0 : state.offset) + end
    return true
}

// Lien vers une partie en cours, suivie en direct par le replay : une icône d'antenne.
export function liveLink(id, title) {
    const a = el('a', { class: 'live-link', href: pageUrl('./', { game: id }), title, 'aria-label': title })
    a.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="2.5" />
        <path d="M7.8 7.8a6 6 0 0 0 0 8.4M16.2 7.8a6 6 0 0 1 0 8.4M4.9 4.9a10 10 0 0 0 0 14.2M19.1 4.9a10 10 0 0 1 0 14.2" /></svg>`
    return a
}

export function setParam(key, value, keepEmpty = false) {
    const url = new URL(location.href)
    if (value || (keepEmpty && value === '')) url.searchParams.set(key, value)
    else url.searchParams.delete(key)
    // Virgules lisibles dans ?teams=.
    url.search = url.searchParams.toString().replaceAll('%2C', ',')
    history.replaceState(null, '', url)
}

// Adresse d'une autre page, avec les choix communs aux pages et les paramètres non vides de `extra`.
export function pageUrl(href, extra = {}) {
    const url = new URL(href, location.href)
    const here = new URLSearchParams(location.search)
    for (const key of ['data', 'session', 'window', 'teams', 'match', 'verified']) {
        if (here.has(key)) url.searchParams.set(key, here.get(key))
    }
    for (const [key, value] of Object.entries(extra)) if (value) url.searchParams.set(key, value)
    url.search = url.searchParams.toString().replaceAll('%2C', ',')
    return url.href
}

const LATEST_RELEASE = 'https://api.github.com/repos/VianAtETS/LOG200-Scoreboard/releases/latest'
const RELEASE_TITLES = {
    current: (tag) => `À jour : dernière release ${tag}`,
    behind: (tag) => `${tag} disponible`,
    ahead: (tag) => `Plus récente que la dernière release (${tag})`,
}

// GitHub limite à 60 appels anonymes par heure et par adresse : un par visite
async function latestRelease() {
    try {
        const cached = sessionStorage.getItem('latest-release')
        if (cached) return cached
    } catch {}
    const response = await fetch(LATEST_RELEASE)
    if (!response.ok) return undefined
    const { tag_name: tag } = await response.json()
    try {
        sessionStorage.setItem('latest-release', tag)
    } catch {}
    return tag
}

export async function showVersion() {
    const response = await fetch(`${SERVER}/api/version`).catch(() => null)
    if (!response?.ok) return
    const version = `v${(await response.text()).trim()}`
    const badge = el('span', { class: 'version', title: 'Version du ladder' }, version)
    document.querySelector('.masthead .title').append(badge)
    const tag = await latestRelease().catch(() => undefined)
    const state = releaseState(version, tag)
    if (state === 'unknown') return
    badge.dataset.release = state
    badge.title = RELEASE_TITLES[state](tag)
    badge.setAttribute('aria-label', `Ladder ${version} : ${badge.title}`)
}

// Liens de l'en-tête : complétés au clic, avec les choix du moment.
export function linkPages() {
    for (const a of document.querySelectorAll('.pages a')) {
        const href = a.getAttribute('href')
        a.addEventListener('click', () => (a.href = pageUrl(href)))
    }
}

// Case "Vérifiées seulement", gardée dans ?verified=1.
export function initVerified(onChange) {
    const box = $('verified')
    box.checked = params.get('verified') === '1'
    box.addEventListener('change', () => {
        setParam('verified', box.checked ? '1' : null)
        onChange()
    })
}

// Classements à afficher. La case n'apparaît qu'une fois une partie distante jouée.
export function scopesOf(summary) {
    $('verified-filter').hidden = !summary.verified
    const verified = Boolean(summary.verified) && $('verified').checked
    return { verified, scopes: verified ? summary.verified : summary.scopes }
}

export function fillSelect(select, values, wanted = select.value) {
    select.replaceChildren(el('option', { value: GLOBAL }, 'Toutes'), ...values.map((v) => el('option', { value: v }, v)))
    select.value = values.includes(wanted) ? wanted : GLOBAL
}

export function pulse() {
    const live = $('live')
    live.classList.remove('pulse')
    void live.offsetWidth
    live.classList.add('pulse')
}
