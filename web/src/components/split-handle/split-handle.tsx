import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type FC,
  type KeyboardEvent,
  type PointerEvent,
} from 'react';

type Props = {
  above: string;
  below: string;
  label: string;
  storageKey: string;
};

const STEP = 24;
const PAGE_STEP = 4 * STEP;
// En-tête du tableau et deux lignes.
const MIN_SCROLL = 112;

const load = (key: string) => {
  try {
    return Number(localStorage.getItem(key)) || null;
  } catch {
    console.warn(`[local-storage] Impossible de charger la hauteur du split : ${key}`);
    return null;
  }
};

const save = (key: string, height: number | null) => {
  try {
    if (height === null) localStorage.removeItem(key);
    else localStorage.setItem(key, String(Math.round(height)));
  } catch {
    console.warn(`[local-storage] Impossible de sauvegarder la hauteur du split : ${key}`);
  }
};

// --split-height sur le gabarit plafonne la section du dessus (.ranking-layout[data-split]).
const SplitHandle: FC<Props> = ({ above, below, label, storageKey }) => {
  const ref = useRef<HTMLDivElement>(null);
  const drag = useRef<{ y: number; height: number } | null>(null);
  const [share, setShare] = useState(50);
  const [dragging, setDragging] = useState(false);

  const nodes = useCallback(() => {
    const top = document.getElementById(above);
    const bottom = document.getElementById(below);
    const layout = ref.current?.parentElement;
    return top && bottom && layout ? { top, bottom, layout } : null;
  }, [above, below]);

  const measure = useCallback(() => {
    const n = nodes();
    if (!n) return;
    const total = n.top.offsetHeight + n.bottom.offsetHeight;
    if (total > 0) setShare(Math.round((100 * n.top.offsetHeight) / total));
  }, [nodes]);

  // Bornée à ce que la grille rend vraiment ; pas au chargement, le tableau est encore vide.
  const apply = useCallback(
    (height: number | null, clamp = true) => {
      const n = nodes();
      if (!n) return null;
      if (height === null) {
        n.layout.style.removeProperty('--split-height');
        delete n.layout.dataset.split;
        return null;
      }
      n.layout.dataset.split = '';
      if (!clamp) {
        n.layout.style.setProperty('--split-height', `${height}px`);
        return height;
      }
      const scroll = n.top.querySelector<HTMLElement>('.scroll');
      const chrome = n.top.offsetHeight - (scroll?.offsetHeight ?? 0);
      const min = chrome + Math.min(scroll?.scrollHeight ?? 0, MIN_SCROLL);
      let next = Math.max(min, height);
      n.layout.style.setProperty('--split-height', `${next}px`);
      next = Math.max(min, Math.min(next, n.top.offsetHeight));
      n.layout.style.setProperty('--split-height', `${next}px`);
      return next;
    },
    [nodes],
  );

  useLayoutEffect(() => {
    apply(load(storageKey), false);
    const n = nodes();
    if (!n) return;
    const observer = new ResizeObserver(measure);
    observer.observe(n.top);
    observer.observe(n.bottom);
    return () => observer.disconnect();
  }, [apply, measure, nodes, storageKey]);

  const current = () => nodes()?.top.offsetHeight ?? 0;
  const commit = (height: number | null) => save(storageKey, apply(height));

  const handlePointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { y: e.clientY, height: current() };
    setDragging(true);
  };

  const handlePointerMove = (e: PointerEvent<HTMLDivElement>) => {
    if (drag.current) apply(drag.current.height + e.clientY - drag.current.y);
  };

  const handlePointerEnd = () => {
    if (!drag.current) return;
    drag.current = null;
    setDragging(false);
    commit(current());
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const moves: Record<string, number | null> = {
      ArrowUp: current() - STEP,
      ArrowDown: current() + STEP,
      PageUp: current() - PAGE_STEP,
      PageDown: current() + PAGE_STEP,
      Home: 0,
      End: 100_000,
      Enter: null,
    };
    if (!(e.key in moves)) return;
    e.preventDefault();
    commit(moves[e.key]);
  };

  return (
    <div
      ref={ref}
      className="split-handle"
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
      onDoubleClick={() => commit(null)}
      onKeyDown={handleKeyDown}
    />
  );
};

export { SplitHandle };
