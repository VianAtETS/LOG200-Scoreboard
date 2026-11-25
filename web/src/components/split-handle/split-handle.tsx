import { useCallback, useLayoutEffect, useRef, useState } from 'react';
import type { FC, KeyboardEvent, PointerEvent, RefObject } from 'react';

type Props = {
  above: string;
  below: string;
  label: string;
  storageKey: string;
};

const STEP = 24;
const PAGE_STEP = 96;
const INITIAL_SHARE = 50;
// En-tête du tableau et deux lignes.
const MIN_SCROLL = 112;
const MAX_SPLIT_HEIGHT = 100_000;

function keyMove(key: string, height: number): number | null | undefined {
  switch (key) {
    case 'ArrowUp': {
      return height - STEP;
    }
    case 'ArrowDown': {
      return height + STEP;
    }
    case 'PageUp': {
      return height - PAGE_STEP;
    }
    case 'PageDown': {
      return height + PAGE_STEP;
    }
    case 'Home': {
      return 0;
    }
    case 'End': {
      return MAX_SPLIT_HEIGHT;
    }
    case 'Enter': {
      return null;
    }
    default: {
      return undefined;
    }
  }
}

const load = (key: string): number | null => {
  try {
    return Number(localStorage.getItem(key)) || null;
  } catch {
    // oxlint-disable-next-line eslint/no-console -- Storage access may be blocked.
    console.warn(`[local-storage] Impossible de charger la hauteur du split : ${key}`);
    return null;
  }
};

const save = (key: string, height: number | null): void => {
  try {
    if (height === null) {
      localStorage.removeItem(key);
    } else {
      localStorage.setItem(key, String(Math.round(height)));
    }
  } catch {
    // oxlint-disable-next-line eslint/no-console -- Storage writes may be blocked.
    console.warn(`[local-storage] Impossible de sauvegarder la hauteur du split : ${key}`);
  }
};

// --split-height sur le gabarit plafonne la section du dessus (.ranking-layout[data-split]).
type SplitHandleState = {
  ref: RefObject<HTMLDivElement | null>;
  share: number;
  dragging: boolean;
  handlePointerDown: (event: PointerEvent<HTMLDivElement>) => void;
  handlePointerMove: (event: PointerEvent<HTMLDivElement>) => void;
  handlePointerEnd: () => void;
  handleKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
  reset: () => void;
};

function useSplitHandle({ above, below, storageKey }: Props): SplitHandleState {
  const ref = useRef<HTMLDivElement>(null);
  const drag = useRef<{ y: number; height: number } | null>(null);
  const [share, setShare] = useState(INITIAL_SHARE);
  const [dragging, setDragging] = useState(false);

  const nodes = useCallback(() => {
    const top = document.querySelector<HTMLElement>(`#${above}`);
    const bottom = document.querySelector<HTMLElement>(`#${below}`);
    const layout = ref.current?.parentElement;
    return top && bottom && layout ? { top, bottom, layout } : null;
  }, [above, below]);

  const measure = useCallback(() => {
    const elements = nodes();
    if (!elements) {
      return;
    }
    const total = elements.top.offsetHeight + elements.bottom.offsetHeight;
    if (total > 0) {
      setShare(Math.round((100 * elements.top.offsetHeight) / total));
    }
  }, [nodes]);

  // Bornée à ce que la grille rend vraiment ; pas au chargement, le tableau est encore vide.
  const apply = useCallback(
    (height: number | null, clamp = true) => {
      const elements = nodes();
      if (!elements) {
        return null;
      }
      if (height === null) {
        elements.layout.style.removeProperty('--split-height');
        delete elements.layout.dataset['split'];
        return null;
      }
      elements.layout.dataset['split'] = '';
      if (!clamp) {
        elements.layout.style.setProperty('--split-height', `${height}px`);
        return height;
      }
      const scroll = elements.top.querySelector<HTMLElement>('.scroll');
      const chrome = elements.top.offsetHeight - (scroll?.offsetHeight ?? 0);
      const min = chrome + Math.min(scroll?.scrollHeight ?? 0, MIN_SCROLL);
      let next = Math.max(min, height);
      elements.layout.style.setProperty('--split-height', `${next}px`);
      next = Math.max(min, Math.min(next, elements.top.offsetHeight));
      elements.layout.style.setProperty('--split-height', `${next}px`);
      return next;
    },
    [nodes],
  );

  useLayoutEffect(() => {
    apply(load(storageKey), false);
    const elements = nodes();
    if (!elements) {
      return () => {
        /* No observer was registered. */
      };
    }
    const observer = new ResizeObserver(measure);
    observer.observe(elements.top);
    observer.observe(elements.bottom);
    return () => observer.disconnect();
  }, [apply, measure, nodes, storageKey]);

  const current = useCallback((): number => nodes()?.top.offsetHeight ?? 0, [nodes]);

  const commit = useCallback(
    (height: number | null): void => save(storageKey, apply(height)),
    [storageKey, apply],
  );

  const handlePointerDown = useCallback(
    (event: PointerEvent<HTMLDivElement>): void => {
      if (event.button !== 0) {
        return;
      }
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      drag.current = { y: event.clientY, height: current() };
      setDragging(true);
    },
    [current],
  );
  const handlePointerMove = useCallback(
    (event: PointerEvent<HTMLDivElement>): void => {
      if (drag.current) {
        apply(drag.current.height + event.clientY - drag.current.y);
      }
    },
    [apply],
  );
  const handlePointerEnd = useCallback((): void => {
    if (!drag.current) {
      return;
    }

    drag.current = null;
    setDragging(false);
    commit(current());
  }, [commit, current]);
  const handleKeyDown = useCallback(
    (ev: KeyboardEvent<HTMLDivElement>): void => {
      const next = keyMove(ev.key, current());
      if (next === undefined) {
        return;
      }
      ev.preventDefault();
      commit(next);
    },
    [commit, current],
  );
  const reset = useCallback((): void => commit(null), [commit]);
  return {
    ref,
    share,
    dragging,
    handlePointerDown,
    handlePointerMove,
    handlePointerEnd,
    handleKeyDown,
    reset,
  };
}

const SplitHandle: FC<Props> = (props) => {
  const { above, below, label } = props;
  const {
    ref,
    share,
    dragging,
    handlePointerDown,
    handlePointerMove,
    handlePointerEnd,
    handleKeyDown,
    reset,
  } = useSplitHandle(props);

  return (
    <div
      ref={ref}
      className="split-handle"
      // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- A focusable splitter is an interactive separator.
      role="separator"
      aria-orientation="horizontal"
      aria-label={label}
      aria-controls={`${above} ${below}`}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={share}
      tabIndex={0}
      data-dragging={dragging || undefined}
      title="Glisser pour partager la hauteur ; double-clic pour revenir à la hauteur par défaut"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerEnd}
      onPointerCancel={handlePointerEnd}
      onDoubleClick={reset}
      onKeyDown={handleKeyDown}
    />
  );
};

export { SplitHandle };
