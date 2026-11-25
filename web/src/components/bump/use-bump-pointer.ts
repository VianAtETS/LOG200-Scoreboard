import { useCallback } from 'react';
import type { PointerEvent, RefObject } from 'react';
import { targetAt } from './bump-inspection';
import type { TGeometry } from './bump.utils';
import type { TPoint } from './use-bump-interaction';

type THover = { col: number | null; team: string | null };
type TPointerHandlers = {
  move: (event: PointerEvent<HTMLDivElement>) => void;
  down: (event: PointerEvent<HTMLDivElement>) => void;
};
type TOptions = {
  plot: RefObject<HTMLDivElement | null>;
  geom: TGeometry | null;
  count: number;
  slider: RefObject<SVGSVGElement | null>;
  frame: RefObject<number | null>;
  pending: RefObject<TPoint | null>;
  cancel: () => void;
  show: (col: number, team?: string | null) => void;
};
export function useBumpPointer(options: TOptions): TPointerHandlers {
  const {
    plot,
    geom,
    count,
    slider: sliderRef,
    frame: frameRef,
    pending: pendingRef,
    cancel,
    show,
  } = options;
  const pointed = useCallback(
    (point: TPoint): THover => {
      const box = plot.current?.getBoundingClientRect();
      return box && geom
        ? targetAt(geom, count, point.clientX - box.left, point.clientY - box.top)
        : { col: null, team: null };
    },
    [plot, geom, count],
  );
  const move = useCallback(
    (event: PointerEvent<HTMLDivElement>): void => {
      if (!geom || !event.isPrimary) {
        return;
      }
      pendingRef.current = { clientX: event.clientX, clientY: event.clientY };
      if (frameRef.current !== null) {
        return;
      }
      frameRef.current = requestAnimationFrame(() => {
        frameRef.current = null;
        const point = pendingRef.current;
        pendingRef.current = null;
        if (!point) {
          return;
        }
        const target = pointed(point);
        if (target.col !== null) {
          show(target.col, target.team);
        }
      });
    },
    [geom, pointed, show, frameRef, pendingRef],
  );
  const down = useCallback(
    (event: PointerEvent<HTMLDivElement>): void => {
      if (
        !geom ||
        !event.isPrimary ||
        event.button !== 0 ||
        (event.target instanceof Element && event.target.closest('.tip-dismiss'))
      ) {
        return;
      }
      cancel();
      sliderRef.current?.focus({ preventScroll: true });
      event.currentTarget.setPointerCapture(event.pointerId);
      const target = pointed(event);
      if (target.col !== null) {
        show(target.col, target.team);
      }
    },
    [geom, cancel, pointed, show, sliderRef],
  );
  return { move, down };
}
