// Champ à puces des équipes (motif ARIA combobox). La page fournit les éléments picker, chips,
// picker-input, picker-clear, picker-toggle et picker-list.

import { $, el } from './common.js'

// `ratings()` : classement de la session (lignes de summary.json, meilleure d'abord).
// `onChange(teams)` : nouvelle sélection, que la page applique puis renvoie par `render`.
export function createPicker({ ratings, onChange }) {
    const field = $('picker-input')
    const list = $('picker-list')
    let selected = []
    let options = []
    let active = -1

    function change(teams) {
        selected = teams
        onChange(teams)
    }

    function add(team) {
        change([...selected, team])
        field.value = ''
        active = -1
        renderOptions()
    }

    const remove = (team) => change(selected.filter((t) => t !== team))

    function renderOptions() {
        const query = field.value.trim().toLowerCase()
        const taken = new Set(selected)
        let rank = 0
        const rows = ratings().map((r) => ({ ...r, rank: r.build ? ++rank : 0 }))
        options = rows.filter((r) => !taken.has(r.team) && r.team.toLowerCase().includes(query))
        active = Math.min(active, options.length - 1)
        const empty = query ? 'Aucune équipe ne correspond.' : 'Toutes les équipes sont choisies.'
        list.replaceChildren(
            ...(options.length
                ? options.map((r, i) => {
                    const option = el(
                        'li',
                        { id: `pick-${i}`, role: 'option', class: 'picker-option', 'aria-selected': String(i === active) },
                        el('span', { class: 'option-rank' }, r.rank ? String(r.rank) : 'bot'),
                        el('span', { class: 'option-team' }, r.team),
                        el('span', { class: 'option-elo' }, `${Math.round(r.rating)} Elo`)
                    )
                    // Garde le focus dans le champ : la liste reste ouverte pour en ajouter d'autres.
                    option.addEventListener('pointerdown', (e) => e.preventDefault())
                    option.addEventListener('click', () => add(r.team))
                    return option
                })
                : [el('li', { class: 'picker-empty' }, empty)])
        )
        if (active >= 0) {
            field.setAttribute('aria-activedescendant', `pick-${active}`)
            $(`pick-${active}`).scrollIntoView({ block: 'nearest' })
        } else field.removeAttribute('aria-activedescendant')
    }

    function openList(open) {
        list.hidden = !open
        field.setAttribute('aria-expanded', String(open))
        if (open) renderOptions()
        else {
            active = -1
            field.removeAttribute('aria-activedescendant')
        }
    }

    field.addEventListener('focus', () => openList(true))
    field.addEventListener('input', () => {
        // Avec une saisie, Entrée ajoute la première équipe qui correspond.
        active = field.value.trim() ? 0 : -1
        openList(true)
    })
    field.addEventListener('keydown', (e) => {
        const open = !list.hidden
        const last = options.length - 1
        if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
            active = !open ? -1 : e.key === 'ArrowDown' ? Math.min(active + 1, last) : Math.max(active - 1, 0)
            openList(true)
        } else if (e.key === 'Enter' && open && active >= 0) add(options[active].team)
        else if (e.key === 'Escape' && open) openList(false)
        else if (e.key === 'Escape') field.value = ''
        else if (e.key === 'Backspace' && !field.value && selected.length) {
            remove(selected.at(-1))
            return
        } else return
        e.preventDefault()
    })
    $('picker').addEventListener('focusout', (e) => {
        if (!$('picker').contains(e.relatedTarget)) openList(false)
    })
    $('picker').addEventListener('click', (e) => {
        if (!e.target.closest('button, li')) field.focus()
    })
    // Les boutons du cadre ne prennent pas le focus : la liste ne se ferme pas sous le clic.
    for (const id of ['picker-toggle', 'picker-clear']) $(id).addEventListener('pointerdown', (e) => e.preventDefault())
    $('picker-toggle').addEventListener('click', () => {
        openList(list.hidden)
        field.focus()
    })
    $('picker-clear').addEventListener('click', () => {
        change([])
        field.focus()
    })

    // `color(team)` : pastille colorée, aucune sans ; `idle(team)` : puce grisée.
    function render(teams, { color, idle } = {}) {
        selected = teams
        $('chips').replaceChildren(
            ...teams.map((team) => {
                const quiet = idle?.(team) ?? false
                const parts = [el('span', {}, team)]
                if (color) {
                    const dot = el('span', { class: 'chip-dot', 'aria-hidden': 'true' })
                    dot.style.background = color(team)
                    parts.unshift(dot)
                }
                const button = el('button', { type: 'button', class: 'chip-remove', 'aria-label': `Retirer ${team}` }, '×')
                button.addEventListener('click', () => remove(team))
                const title = quiet ? `${team} : aucune partie dans cette fenêtre` : team
                return el('li', { class: `chip${quiet ? ' idle' : ''}`, title }, ...parts, button)
            })
        )
        $('picker-clear').hidden = !teams.length
        if (!list.hidden) renderOptions()
    }

    return { render }
}
