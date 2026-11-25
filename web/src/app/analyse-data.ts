import { useGetGame } from '@/dao/hooks/use-games';
import { useGetSummary } from '@/dao/hooks/use-ladder';
import { useSearchParam } from '@/hooks/use-search-param';
import { RED } from '@/lib/replay';
import type { TStep } from '@/lib/replay';
import type { TColor, TGame } from '@/types/games.types';
import { useCallback, useEffect, useState } from 'react';
import type { Point, Timed } from './analyse-views';

const COLORS = ['red', 'black'] as const;

type TMoveData = {
  timed: Timed[];
  depths: Point[];
  rates: Point[];
  moverOf: (index: number) => TColor | undefined;
};

export function moveData(game: TGame | undefined, steps: TStep[]): TMoveData {
  // Temps des coups des équipes, après l'ouverture ; vide s'il manque ou ne correspond pas aux coups.
  const moveMs = game?.move_ms?.length === game?.moves.length ? (game?.move_ms ?? []) : [];
  const offset = steps.length - moveMs.length;
  const moverOf = (i: number): TColor | undefined => {
    const position = steps[i];
    if (position?.move) {
      return position.cells[position.move.to] === RED ? 'red' : 'black';
    }
    return undefined;
  };
  const timed = moveMs.flatMap((ms, index) => {
    const at = offset + index;
    const color = moverOf(at);
    return color ? [{ ms, at, color }] : [];
  });
  // Mesures des bots (lignes ARENA), chacune à la position que son coup produit.
  const firstMove = steps.length - (game?.moves.length ?? 0);
  const measured = COLORS.flatMap((color) => {
    const at: number[] = [];
    for (let i = Math.max(firstMove, 1); i < steps.length; i += 1) {
      if (moverOf(i) === color) {
        at.push(i);
      }
    }
    return (game?.measures?.[color] ?? []).flatMap((measurement, index) => {
      const position = at[index];
      if (position === undefined) {
        return [];
      }
      const ms = game?.move_ms?.[position - firstMove];
      return [
        {
          color,
          at: position,
          depth: measurement.depth,
          rate: ms !== undefined && ms !== 0 ? (1000 * measurement.nodes) / ms : undefined,
        },
      ];
    });
  });
  const depths = measured.flatMap((measurement) =>
    measurement.depth === undefined
      ? []
      : [{ color: measurement.color, at: measurement.at, value: measurement.depth }],
  );
  const rates = measured.flatMap((measurement) =>
    measurement.rate === undefined
      ? []
      : [{ color: measurement.color, at: measurement.at, value: measurement.rate }],
  );
  return { timed, depths, rates, moverOf };
}

type TAnalysisNavigation = {
  last: number;
  k: number;
  go: (index: number) => void;
  prevCrit: number | undefined;
  nextCrit: number | undefined;
};

export function useAnalysisNavigation(stepCount: number, crit: number[]): TAnalysisNavigation {
  const last = stepCount - 1;
  // null : la fin de la partie.
  const [chosen, setChosen] = useState<number | null>(null);
  const k = chosen === null ? last : Math.min(chosen, last);
  const go = useCallback((i: number) => setChosen(Math.max(0, Math.min(last, i))), [last]);
  const prevCrit = crit.findLast((color) => color < k);
  const nextCrit = crit.find((color) => color > k);
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.target instanceof Element && event.target.closest('input, select, textarea')) {
        return;
      }
      const action = {
        ArrowLeft: () => go(k - 1),
        ArrowRight: () => go(k + 1),
        ArrowUp: () => prevCrit !== undefined && go(prevCrit),
        ArrowDown: () => nextCrit !== undefined && go(nextCrit),
        Home: () => go(0),
        End: () => go(last),
      }[event.key];
      if (!action) {
        return;
      }
      event.preventDefault();
      action();
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [go, k, last, prevCrit, nextCrit]);
  return { last, k, go, prevCrit, nextCrit };
}

export function useAnalysisSources(): {
  summaryQuery: ReturnType<typeof useGetSummary>;
  gameQuery: ReturnType<typeof useGetGame>;
  gameParam: string | null;
  row: Parameters<typeof useGetGame>[0];
} {
  const summaryQuery = useGetSummary();
  const [gameParam] = useSearchParam('game');
  const row =
    gameParam !== null && gameParam !== ''
      ? { id: gameParam }
      : summaryQuery.data?.scopes['']?.recent.at(-1);
  const gameQuery = useGetGame(row);
  return { summaryQuery, gameQuery, gameParam, row };
}
