// Serveur : la balise meta scoreboard-server (posée par le workflow GitHub Pages), sinon le site qui
// sert la page. Données : ?data=URL du dossier, sinon <serveur>/data.

import { parseJsonl } from './elo.js'

export const params = new URLSearchParams(location.search)
export const SERVER = document.querySelector('meta[name="scoreboard-server"]')?.content || '..'
export const DATA = params.get('data') ?? `${SERVER}/data`
export const REFRESH_MS = 15000
export const GLOBAL = ''
// Fenêtres des graphes en ms ; une valeur absente vaut « Tout ».
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
    for (const key of ['data', 'session', 'window', 'teams', 'match']) {
        if (here.has(key)) url.searchParams.set(key, here.get(key))
    }
    for (const [key, value] of Object.entries(extra)) if (value) url.searchParams.set(key, value)
    url.search = url.searchParams.toString().replaceAll('%2C', ',')
    return url.href
}

// Liens de l'en-tête : complétés au clic, avec les choix du moment.
export function linkPages() {
    for (const a of document.querySelectorAll('.pages a')) {
        const href = a.getAttribute('href')
        a.addEventListener('click', () => (a.href = pageUrl(href)))
    }
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
