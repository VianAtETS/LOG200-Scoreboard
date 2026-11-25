import type { TRankPoint } from '@/lib/bump';
import type { TRatingRow } from '@/types/games.types';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { FocusEvent, KeyboardEvent, MouseEvent, PointerEvent, RefObject } from 'react';
import { clampColumn, standingsAt } from './bump-inspection';
import type { TStandingRow } from './bump-inspection';
import { useBumpKey } from './use-bump-key';
import { useBumpPointer } from './use-bump-pointer';
import type { TChartMetric, TGeometry } from './bump.utils';

type TInteraction = {
  figure: RefObject<HTMLElement | null>;
  slider: RefObject<SVGSVGElement | null>;
  inspection: { col: number; team: string | null; rows: TStandingRow[] } | null;
  hide: () => void;
  move: (event: PointerEvent<HTMLDivElement>) => void;
  down: (event: PointerEvent<HTMLDivElement>) => void;
  key: (event: KeyboardEvent<SVGSVGElement>) => void;
  focus: () => void;
  leave: (event: PointerEvent<HTMLElement>) => void;
  blur: (event: FocusEvent<HTMLElement>) => void;
  contextMenu: (event: MouseEvent<HTMLDivElement>) => void;
};
type THover = { col: number | null; team: string | null };
export type TPoint = { clientX: number; clientY: number };

function contextMenu(event: MouseEvent<HTMLDivElement>): void {
  event.preventDefault();
}

export function useBumpInteraction(
  plot: RefObject<HTMLDivElement | null>,
  geom: TGeometry | null,
  columns: TRatingRow[],
  series: Map<string, TRankPoint[]>,
  metric: TChartMetric,
): TInteraction {
  const figure = useRef<HTMLElement>(null);
  const slider = useRef<SVGSVGElement>(null);
  const frame = useRef<number | null>(null);
  const pending = useRef<TPoint | null>(null);
  const [hover, setHover] = useState<THover>({ col: null, team: null });
  const cancel = useCallback((): void => {
    if (frame.current !== null) {
      cancelAnimationFrame(frame.current);
    }
    frame.current = null;
    pending.current = null;
  }, []);
  useLayoutEffect(() => {
    if (geom && columns.length > 0) {
      cancel();
    }
    return cancel;
  }, [cancel, geom, columns]);
  const hide = useCallback((): void => {
    cancel();
    setHover((previous) => (previous.col === null ? previous : { ...previous, col: null }));
  }, [cancel]);
  const inspect = useCallback(
    (requested: number, requestedTeam: string | null) => {
      const col = clampColumn(requested, columns.length);
      const rows = standingsAt(geom?.drawn ?? [], series, metric, col);
      const team = rows.some((row) => row.team === requestedTeam)
        ? requestedTeam
        : (rows[0]?.team ?? null);
      return { col, rows, team };
    },
    [columns.length, geom, series, metric],
  );
  const show = useCallback(
    (col: number, requestedTeam?: string | null): void => {
      const team = requestedTeam === undefined ? hover.team : requestedTeam;
      if (!geom) {
        hide();
        return;
      }
      cancel();
      const next = inspect(col, team);
      setHover((previous) =>
        previous.col === next.col && previous.team === next.team
          ? previous
          : { col: next.col, team: next.team },
      );
    },
    [geom, hide, cancel, inspect, hover.team],
  );
  const inspection = geom && hover.col !== null ? inspect(hover.col, hover.team) : null;
  const col = inspection?.col ?? null;
  const isOpen = hover.col !== null;
  useEffect(() => {
    const outside = (event: globalThis.PointerEvent): void => {
      if (event.target instanceof Node && figure.current?.contains(event.target)) {
        return;
      }
      hide();
    };
    if (isOpen) {
      document.addEventListener('pointerdown', outside);
    }
    return (): void => {
      document.removeEventListener('pointerdown', outside);
    };
  }, [isOpen, hide]);
  const { move, down } = useBumpPointer({
    plot,
    geom,
    count: columns.length,
    slider,
    frame,
    pending,
    cancel,
    show,
  });
  const key = useBumpKey({
    geom,
    col,
    columns,
    show,
    series,
    metric,
    team: hover.team,
    cancel,
    hide,
  });
  const focus = useCallback((): void => {
    if (geom) {
      show(hover.col ?? columns.length - 1);
    }
  }, [geom, show, hover.col, columns.length]);
  const leave = useCallback(
    (event: PointerEvent<HTMLElement>): void => {
      if (!event.currentTarget.contains(document.activeElement)) {
        hide();
      }
    },
    [hide],
  );
  const blur = useCallback(
    (event: FocusEvent<HTMLElement>): void => {
      if (event.relatedTarget && !event.currentTarget.contains(event.relatedTarget)) {
        hide();
      }
    },
    [hide],
  );
  return { figure, slider, inspection, hide, move, down, key, focus, leave, blur, contextMenu };
}
