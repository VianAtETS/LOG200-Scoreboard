import { ChevronDownIcon, CloseIcon } from '@/components/icons/icons';
import { cn } from '@/lib/utils';
import type { TTeamRating } from '@/types/summary.types';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type {
  ChangeEvent,
  FC,
  FocusEvent,
  KeyboardEvent,
  MouseEvent,
  PointerEventHandler,
  RefObject,
} from 'react';

type Props = {
  // Classement de la session, meilleure d'abord.
  ratings: TTeamRating[];
  selected: string[];
  onChange: (teams: string[]) => void;
  placeholder: string;
  chipsLabel: string;
  className?: string | undefined;
  color?: ((team: string) => string) | undefined;
  idle?: ((team: string) => boolean) | undefined;
};

// On iOS, cancelling touch pointerdown or closing on a null blur can prevent option activation.
const keepMouseFocus: PointerEventHandler<HTMLElement> = (event) => {
  if (event.pointerType === 'mouse') {
    event.preventDefault();
  }
};

type TOption = TTeamRating & { rank: number };

function teamOptions(ratings: TTeamRating[], selected: string[], search: string): TOption[] {
  const taken = new Set(selected);
  let rank = 0;
  const options: TOption[] = [];
  for (const rating of ratings) {
    if (rating.build !== undefined && rating.build !== '') {
      rank += 1;
    }
    if (!taken.has(rating.team) && rating.team.toLowerCase().includes(search)) {
      options.push({
        ...rating,
        rank: rating.build !== undefined && rating.build !== '' ? rank : 0,
      });
    }
  }
  return options;
}

type TPicker = {
  root: RefObject<HTMLDivElement | null>;
  field: RefObject<HTMLInputElement | null>;
  list: RefObject<HTMLUListElement | null>;
  query: string;
  search: string;
  current: number;
  open: boolean;
  options: TOption[];
  handleBlur: (event: FocusEvent<HTMLDivElement>) => void;
  handleFieldClick: (event: MouseEvent<HTMLDivElement>) => void;
  handleKeyDown: (event: KeyboardEvent<HTMLInputElement>) => void;
  handleFocus: () => void;
  handleInput: (event: ChangeEvent<HTMLInputElement>) => void;
  handleClear: () => void;
  handleToggle: () => void;
  remove: (event: MouseEvent<HTMLButtonElement>) => void;
  handlePick: (event: MouseEvent<HTMLLIElement>) => void;
};

function usePickerFocus(
  root: TPicker['root'],
  list: TPicker['list'],
  open: boolean,
  current: number,
  close: () => void,
): void {
  useEffect(() => {
    const closeOutside = (event: PointerEvent): void => {
      if (event.target instanceof Node && !root.current?.contains(event.target)) {
        close();
      }
    };
    document.addEventListener('pointerdown', closeOutside);
    return () => document.removeEventListener('pointerdown', closeOutside);
  }, [root, close]);
  useLayoutEffect(() => {
    if (open && current >= 0) {
      list.current?.querySelector(`#pick-${current}`)?.scrollIntoView({ block: 'nearest' });
    }
  }, [open, current, list]);
}

function nextOption(key: 'ArrowDown' | 'ArrowUp', current: number, last: number): number {
  return key === 'ArrowDown' ? Math.min(current + 1, last) : Math.max(current - 1, 0);
}

function usePicker(
  ratings: TTeamRating[],
  selected: string[],
  onChange: Props['onChange'],
): TPicker {
  const root = useRef<HTMLDivElement>(null);
  const field = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);

  const search = query.trim().toLowerCase();
  const options = teamOptions(ratings, selected, search);
  const current = Math.min(active, options.length - 1);

  const close = useCallback((): void => {
    setOpen(false);
    setActive(-1);
  }, []);

  usePickerFocus(root, list, open, current, close);

  const openList = (next: boolean): void => {
    setOpen(next);
    if (!next) {
      setActive(-1);
    }
  };

  const add = (team: string): void => {
    onChange([...selected, team]);
    setQuery('');
    setActive(-1);
  };

  const remove = (team: string): void => {
    onChange(selected.filter((item) => item !== team));
    field.current?.focus();
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>): void => {
    if (event.nativeEvent.isComposing) {
      return;
    }
    const last = options.length - 1;
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      setActive(open ? nextOption(event.key, current, last) : -1);
      setOpen(true);
    } else if (event.key === 'Enter' && open && current >= 0) {
      const option = options[current];
      if (!option) {
        return;
      }
      add(option.team);
    } else if (event.key === 'Escape' && open) {
      openList(false);
    } else if (event.key === 'Escape') {
      setQuery('');
    } else if (event.key === 'Backspace' && query === '') {
      const lastSelected = selected.at(-1);
      if (lastSelected !== undefined) {
        remove(lastSelected);
      }
      return;
    } else {
      return;
    }
    event.preventDefault();
  };
  const handleBlur = (event: FocusEvent<HTMLDivElement>): void => {
    if (event.relatedTarget && !event.currentTarget.contains(event.relatedTarget)) {
      openList(false);
    }
  };
  const handleFieldClick = (event: MouseEvent<HTMLDivElement>): void => {
    if (!(event.target instanceof Element) || !event.target.closest('button, li')) {
      field.current?.focus();
    }
  };
  const handleFocus = (): void => openList(true);
  const handleInput = (event: ChangeEvent<HTMLInputElement>): void => {
    const { value } = event.currentTarget;
    setQuery(value);
    setActive(value.trim() ? 0 : -1);
    setOpen(true);
  };
  const handleClear = (): void => {
    onChange([]);
    field.current?.focus();
  };

  const handleToggle = (): void => {
    if (open) {
      openList(false);
    } else {
      field.current?.focus();
      openList(true);
    }
  };
  const handlePick = (event: MouseEvent<HTMLLIElement>): void => {
    const { team } = event.currentTarget.dataset;
    if (team !== undefined) {
      add(team);
    }
  };
  return {
    root,
    field,
    list,
    search,
    options,
    current,
    open,
    query,
    handleBlur,
    handleFieldClick,
    handleKeyDown,
    handleFocus,
    handleInput,
    handleClear,
    handleToggle,
    handlePick,
    remove: (event: MouseEvent<HTMLButtonElement>): void => remove(event.currentTarget.value),
  };
}

function renderChips(
  selected: string[],
  chipsLabel: string,
  color: Props['color'],
  idle: Props['idle'],
  remove: TPicker['remove'],
): React.JSX.Element {
  return (
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
              // oxlint-disable-next-line react-perf/jsx-no-new-object-as-prop -- A per-team native colour swatch does not benefit from memoization.
              <span className="chip-dot" aria-hidden="true" style={{ background: color(team) }} />
            )}
            <span>{team}</span>
            <button
              type="button"
              className="chip-remove"
              aria-label={`Retirer ${team}`}
              value={team}
              onClick={remove}
            >
              ×
            </button>
          </li>
        );
      })}
    </ul>
  );
}

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
  const picker = usePicker(ratings, selected, onChange);
  const { root, field, list, search, options, open, current, query } = picker;
  const empty = search ? 'Aucune équipe ne correspond.' : 'Toutes les équipes sont choisies.';

  return (
    // oxlint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions -- The blank background delegates pointer focus to the input; Tab reaches the input directly.
    <div
      ref={root}
      id="picker"
      className={cn('picker', className)}
      onBlur={picker.handleBlur}
      onClick={picker.handleFieldClick}
    >
      {renderChips(selected, chipsLabel, color, idle, picker.remove)}
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
        onFocus={picker.handleFocus}
        onChange={picker.handleInput}
        onKeyDown={picker.handleKeyDown}
      />
      <button
        id="picker-clear"
        className="picker-icon"
        type="button"
        aria-label="Retirer toutes les équipes"
        hidden={selected.length === 0}
        onPointerDown={keepMouseFocus}
        onClick={picker.handleClear}
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
        onClick={picker.handleToggle}
      >
        <ChevronDownIcon />
      </button>
      <ul
        ref={list}
        id="picker-list"
        className="picker-list"
        // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role, jsx-a11y/no-noninteractive-element-to-interactive-role -- An editable combobox keeps focus on its input while controlling this custom listbox.
        role="listbox"
        aria-label="Équipes"
        hidden={!open}
      >
        {open &&
          options.length > 0 &&
          options.map((rating, index) => (
            // oxlint-disable-next-line jsx-a11y/click-events-have-key-events -- Enter is handled on the combobox through aria-activedescendant.
            <li
              key={rating.team}
              id={`pick-${index}`}
              // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role, jsx-a11y/no-noninteractive-element-to-interactive-role -- Each option carries multiple formatted fields inside the controlled listbox.
              role="option"
              className="picker-option"
              aria-selected={index === current}
              onPointerDown={keepMouseFocus}
              data-team={rating.team}
              onClick={picker.handlePick}
            >
              <span className="option-rank">{rating.rank ? String(rating.rank) : 'bot'}</span>
              <span className="option-team">{rating.team}</span>
              <span className="option-elo">{Math.round(rating.rating)} Elo</span>
            </li>
          ))}
        {open && options.length === 0 && (
          <li className="picker-empty" role="presentation">
            {empty}
          </li>
        )}
      </ul>
    </div>
  );
};

export { TeamPicker };
