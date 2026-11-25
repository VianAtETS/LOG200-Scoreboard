import { ChevronDownIcon, CloseIcon } from '@/components/icons/icons';
import { cn } from '@/lib/utils';
import type { TTeamRating } from '@/types/summary.types';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { FC, KeyboardEvent, PointerEventHandler } from 'react';

type Props = {
  // Classement de la session, meilleure d'abord.
  ratings: TTeamRating[];
  selected: string[];
  onChange: (teams: string[]) => void;
  placeholder: string;
  chipsLabel: string;
  className?: string;
  color?: (team: string) => string;
  idle?: (team: string) => boolean;
};

// On iOS, cancelling touch pointerdown or closing on a null blur can prevent option activation.
const keepMouseFocus: PointerEventHandler<HTMLElement> = (e) => {
  if (e.pointerType === 'mouse') {
    e.preventDefault();
  }
};

const TeamPicker: FC<Props> = ({
  ratings,
  selected,
  onChange,
  placeholder,
  chipsLabel,
  className,
  color,
  idle,
}) => {
  const root = useRef<HTMLDivElement>(null);
  const field = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);

  const search = query.trim().toLowerCase();
  const taken = new Set(selected);
  let rank = 0;
  const options = ratings
    .map((r) => ({ ...r, rank: r.build ? ++rank : 0 }))
    .filter((r) => !taken.has(r.team) && r.team.toLowerCase().includes(search));
  const current = Math.min(active, options.length - 1);

  useEffect(() => {
    const closeOutside = (e: PointerEvent) => {
      if (e.target instanceof Node && !root.current?.contains(e.target)) {
        setOpen(false);
        setActive(-1);
      }
    };
    document.addEventListener('pointerdown', closeOutside);
    return () => document.removeEventListener('pointerdown', closeOutside);
  }, []);

  useLayoutEffect(() => {
    if (open && current >= 0) {
      list.current?.querySelector(`#pick-${current}`)?.scrollIntoView({ block: 'nearest' });
    }
  }, [open, current]);

  const openList = (next: boolean) => {
    setOpen(next);
    if (!next) {
      setActive(-1);
    }
  };

  const add = (team: string) => {
    onChange([...selected, team]);
    setQuery('');
    setActive(-1);
  };

  const remove = (team: string) => {
    onChange(selected.filter((t) => t !== team));
    field.current?.focus();
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    const last = options.length - 1;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      setActive(
        open
          ? e.key === 'ArrowDown'
            ? Math.min(current + 1, last)
            : Math.max(current - 1, 0)
          : -1,
      );
      setOpen(true);
    } else if (e.key === 'Enter' && open && current >= 0) {
      add(options[current].team);
    } else if (e.key === 'Escape' && open) {
      openList(false);
    } else if (e.key === 'Escape') {
      setQuery('');
    } else if (e.key === 'Backspace' && !query && selected.length > 0) {
      remove(selected.at(-1));
      return;
    } else {
      return;
    }
    e.preventDefault();
  };

  const empty = search ? 'Aucune équipe ne correspond.' : 'Toutes les équipes sont choisies.';

  return (
    <div
      ref={root}
      id="picker"
      className={cn('picker', className)}
      onBlur={(e) => {
        if (e.relatedTarget && !e.currentTarget.contains(e.relatedTarget)) {
          openList(false);
        }
      }}
      onClick={(e) => {
        if (!(e.target as Element).closest('button, li')) {
          field.current?.focus();
        }
      }}
    >
      <ul id="chips" className="chips" aria-label={chipsLabel}>
        {selected.map((team) => {
          const quiet = idle?.(team) ?? false;
          return (
            <li
              key={team}
              className={cn('chip', quiet && 'idle')}
              title={quiet ? `${team} : aucune partie dans cette fenêtre` : team}
            >
              {color && (
                <span className="chip-dot" aria-hidden="true" style={{ background: color(team) }} />
              )}
              <span>{team}</span>
              <button
                type="button"
                className="chip-remove"
                aria-label={`Retirer ${team}`}
                onClick={() => remove(team)}
              >
                ×
              </button>
            </li>
          );
        })}
      </ul>
      <input
        ref={field}
        id="picker-input"
        className="picker-input"
        type="text"
        role="combobox"
        aria-label="Ajouter une équipe"
        aria-expanded={open}
        aria-controls="picker-list"
        aria-autocomplete="list"
        aria-activedescendant={open && current >= 0 ? `pick-${current}` : undefined}
        placeholder={placeholder}
        autoComplete="off"
        spellCheck={false}
        value={query}
        onFocus={() => openList(true)}
        onChange={(e) => {
          setQuery(e.target.value);
          // Avec une saisie, Entrée ajoute la première équipe qui correspond.
          setActive(e.target.value.trim() ? 0 : -1);
          setOpen(true);
        }}
        onKeyDown={handleKeyDown}
      />
      <button
        id="picker-clear"
        className="picker-icon"
        type="button"
        aria-label="Retirer toutes les équipes"
        hidden={selected.length === 0}
        onPointerDown={keepMouseFocus}
        onClick={() => {
          onChange([]);
          field.current?.focus();
        }}
      >
        <CloseIcon />
      </button>
      <button
        id="picker-toggle"
        className="picker-icon"
        type="button"
        aria-label="Afficher les équipes"
        aria-expanded={open}
        aria-controls="picker-list"
        onPointerDown={keepMouseFocus}
        onClick={() => {
          if (open) {
            openList(false);
          } else {
            field.current?.focus();
            openList(true);
          }
        }}
      >
        <ChevronDownIcon />
      </button>
      <ul
        ref={list}
        id="picker-list"
        className="picker-list"
        role="listbox"
        aria-label="Équipes"
        hidden={!open}
      >
        {open ? (
          options.length ? (
            options.map((r, i) => (
              <li
                key={r.team}
                id={`pick-${i}`}
                role="option"
                className="picker-option"
                aria-selected={i === current}
                onPointerDown={keepMouseFocus}
                onClick={() => add(r.team)}
              >
                <span className="option-rank">{r.rank ? String(r.rank) : 'bot'}</span>
                <span className="option-team">{r.team}</span>
                <span className="option-elo">{Math.round(r.rating)} Elo</span>
              </li>
            ))
          ) : (
            <li className="picker-empty">{empty}</li>
          )
        ) : null}
      </ul>
    </div>
  );
};

export { TeamPicker };
