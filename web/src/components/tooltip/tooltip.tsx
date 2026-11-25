import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type FC,
  type FocusEvent,
  type PointerEvent,
  type PropsWithChildren,
  type RefObject,
} from 'react';
import { createPortal } from 'react-dom';

const GAP = 8;
const MARGIN = 8;

type Props = PropsWithChildren & {
  id: string;
  anchor: HTMLElement | null;
  pinned: boolean;
  tipRef: RefObject<HTMLDivElement | null>;
};

// Portail pour pas qu'il soit coupé par un overflow: hidden.
const Tooltip: FC<Props> = ({ id, anchor, pinned, tipRef, children }) => {
  useLayoutEffect(() => {
    const tip = tipRef.current;
    if (!anchor || !tip) return;
    const a = anchor.getBoundingClientRect();
    const t = tip.getBoundingClientRect();
    const below = a.top - GAP - t.height < MARGIN;
    const left = Math.min(
      Math.max(a.left + a.width / 2 - t.width / 2, MARGIN),
      innerWidth - MARGIN - t.width,
    );
    tip.style.left = `${left}px`;
    tip.style.top = `${below ? a.bottom + GAP : a.top - GAP - t.height}px`;
    tip.dataset.side = below ? 'below' : 'above';
  }, [anchor, pinned, tipRef, children]);

  if (!anchor) return null;
  return createPortal(
    <div
      ref={tipRef}
      id={id}
      className={pinned ? 'tooltip pinned' : 'tooltip'}
      role={pinned ? 'dialog' : 'tooltip'}
    >
      {children}
    </div>,
    document.body,
  );
};

type TState<T> = { anchor: T; pinned: boolean } | null;

// pin() : au tactile, sans survol, la bulle reste ouverte jusqu'à un tap ailleurs.
const useTooltip = <T extends HTMLElement>() => {
  const id = useId();
  const tipRef = useRef<HTMLDivElement>(null);
  const [state, setState] = useState<TState<T>>(null);
  const hide = useCallback(() => setState(null), []);
  const unpinned = useCallback(() => setState((s) => (s?.pinned ? s : null)), []);

  useEffect(() => {
    if (!state) return;
    const handleKeyDown = (e: KeyboardEvent) => e.key === 'Escape' && hide();
    const handlePointerDown = (e: globalThis.PointerEvent) => {
      const target = e.target as Node;
      if (!state.anchor.contains(target) && !tipRef.current?.contains(target)) hide();
    };
    addEventListener('scroll', hide, { capture: true, passive: true });
    addEventListener('keydown', handleKeyDown);
    if (state.pinned) addEventListener('pointerdown', handlePointerDown);
    return () => {
      removeEventListener('scroll', hide, { capture: true });
      removeEventListener('keydown', handleKeyDown);
      removeEventListener('pointerdown', handlePointerDown);
    };
  }, [state, hide]);

  const show = (anchor: T) => setState((s) => (s?.pinned ? s : { anchor, pinned: false }));

  const pin = (anchor: T) => {
    if (state?.pinned && state.anchor === anchor) {
      setState(null);
      anchor.blur();
      return;
    }
    setState({ anchor, pinned: true });
    anchor.focus({ preventScroll: true });
  };

  const anchorProps = {
    'aria-describedby': id,
    // Un title vide coupe aussi celui, natif, de la cellule.
    title: '',
    onPointerEnter: (e: PointerEvent<T>) => e.pointerType === 'mouse' && show(e.currentTarget),
    onPointerLeave: (e: PointerEvent<T>) => e.pointerType === 'mouse' && unpinned(),
    onFocus: (e: FocusEvent<T>) => show(e.currentTarget),
    onBlur: unpinned,
  };

  const tipProps = { id, anchor: state?.anchor ?? null, pinned: state?.pinned ?? false, tipRef };
  return { anchorProps, tipProps, pin };
};

export { Tooltip, useTooltip };
