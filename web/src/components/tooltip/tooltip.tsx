import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import type { FC, FocusEvent, PointerEvent, PropsWithChildren, RefObject } from 'react';
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
    if (!anchor || !tip) {
      return;
    }
    const anchorRect = anchor.getBoundingClientRect();
    const tipRect = tip.getBoundingClientRect();
    const below = anchorRect.top - GAP - tipRect.height < MARGIN;
    const left = Math.min(
      Math.max(anchorRect.left + anchorRect.width / 2 - tipRect.width / 2, MARGIN),
      innerWidth - MARGIN - tipRect.width,
    );
    tip.style.left = `${left}px`;
    tip.style.top = `${below ? anchorRect.bottom + GAP : anchorRect.top - GAP - tipRect.height}px`;
    tip.dataset['side'] = below ? 'below' : 'above';
  });
  if (!anchor || typeof document === 'undefined') {
    return null;
  }

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
type TooltipResult<T extends HTMLElement> = {
  anchorProps: {
    'aria-describedby': string;
    title: string;
    onPointerEnter: (event: PointerEvent<T>) => void;
    onPointerLeave: (event: PointerEvent<T>) => void;
    onFocus: (event: FocusEvent<T>) => void;
    onBlur: () => void;
  };
  tipProps: Omit<Props, 'children'>;
  pin: (anchor: T) => void;
};

const useTooltip = <T extends HTMLElement>(): TooltipResult<T> => {
  const id = useId();
  const tipRef = useRef<HTMLDivElement>(null);
  const [state, setState] = useState<TState<T>>(null);
  const hide = useCallback((): void => {
    setState(null);
  }, []);
  const unpinned = useCallback((): void => {
    setState((previous) => (previous?.pinned ? previous : null));
  }, []);

  useEffect(() => {
    if (!state) {
      return () => {
        /* No listeners were registered. */
      };
    }
    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        hide();
      }
    };
    const handlePointerDown = (ev: globalThis.PointerEvent): void => {
      if (
        !(ev.target instanceof Node) ||
        (!state.anchor.contains(ev.target) && !tipRef.current?.contains(ev.target))
      ) {
        hide();
      }
    };
    addEventListener('scroll', hide, { capture: true, passive: true });
    addEventListener('keydown', handleKeyDown);
    if (state.pinned) {
      addEventListener('pointerdown', handlePointerDown);
    }
    return () => {
      removeEventListener('scroll', hide, { capture: true });
      removeEventListener('keydown', handleKeyDown);
      removeEventListener('pointerdown', handlePointerDown);
    };
  }, [state, hide]);

  const show = (anchor: T): void => {
    setState((previous) => (previous?.pinned ? previous : { anchor, pinned: false }));
  };

  const pin = (anchor: T): void => {
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
    onPointerEnter: (event: PointerEvent<T>): void => {
      if (event.pointerType === 'mouse') {
        show(event.currentTarget);
      }
    },
    onPointerLeave: (event: PointerEvent<T>): void => {
      if (event.pointerType === 'mouse') {
        unpinned();
      }
    },
    onFocus: (event: FocusEvent<T>): void => show(event.currentTarget),
    onBlur: unpinned,
  };

  const tipProps = { id, anchor: state?.anchor ?? null, pinned: state?.pinned ?? false, tipRef };
  return { anchorProps, tipProps, pin };
};

// The hook and component form one tooltip API.
// oxlint-disable-next-line react/only-export-components
export { Tooltip, useTooltip };
