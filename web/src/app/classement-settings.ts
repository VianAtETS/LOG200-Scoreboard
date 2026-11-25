import {
  DEFAULT_STATS,
  ELO_STATS,
  PACE_STATS,
  RECORD_STATS,
  sortOf,
  statOf,
} from '@/components/ranking/ranking.data';
import type { TEloStat, TRankingSort, TRecordStat } from '@/components/ranking/ranking.data';
import { useSearchParam } from '@/hooks/use-search-param';
import { setParam } from '@/lib/url';
import type { TPaceStat } from '@/types/summary.types';
import { useCallback, useState } from 'react';
import type { ChangeEvent } from 'react';

type TSetParam = (value: string | null) => void;

type TReplayFilters = {
  setChosen: (teams: string[]) => void;
  setSessionChoice: TSetParam;
  setVerifiedParam: TSetParam;
  setMatchParam: TSetParam;
  setSideParam: TSetParam;
  unpin: () => void;
};

type TFilterHandlers = {
  chooseTeams: (teams: string[]) => void;
  changeSession: (value: string) => void;
  changeVerified: (value: boolean) => void;
  changeMatch: (value: string) => void;
  changeSide: (event: ChangeEvent<HTMLSelectElement>) => void;
};

export function useReplayFilters({
  setChosen,
  setSessionChoice,
  setVerifiedParam,
  setMatchParam,
  setSideParam,
  unpin,
}: TReplayFilters): TFilterHandlers {
  const chooseTeams = useCallback(
    (next: string[]): void => {
      setChosen(next);
      setParam('teams', next.join(','));
      unpin();
    },
    [setChosen, unpin],
  );
  const changeSession = useCallback(
    (value: string): void => {
      setSessionChoice(value);
      unpin();
    },
    [setSessionChoice, unpin],
  );
  const changeVerified = useCallback(
    (checked: boolean): void => {
      setVerifiedParam(checked ? '1' : null);
      unpin();
    },
    [setVerifiedParam, unpin],
  );
  const changeMatch = useCallback(
    (value: string): void => {
      setMatchParam(value);
      unpin();
    },
    [setMatchParam, unpin],
  );
  const changeSide = useCallback(
    (event: ChangeEvent<HTMLSelectElement>): void => {
      const { value } = event.currentTarget;
      setSideParam(value === 'color' ? null : value);
    },
    [setSideParam],
  );
  return { chooseTeams, changeSession, changeVerified, changeMatch, changeSide };
}

function useRankingStat<T extends string>(
  param: string,
  options: Record<T, readonly [label: string, title: string]>,
  fallback: T,
): readonly [T, (value: T) => void] {
  const [value, setValue] = useSearchParam(param);
  const update = useCallback(
    (next: T): void => setValue(next === fallback ? null : next),
    [setValue, fallback],
  );
  return [statOf(options, value, fallback), update];
}

type TSettings = {
  eloStat: TEloStat;
  recordStat: TRecordStat;
  paceStat: TPaceStat;
  sort: TRankingSort;
  changeElo: (value: TEloStat) => void;
  changeRecord: (value: TRecordStat) => void;
  changePace: (value: TPaceStat) => void;
  changeSort: (value: TRankingSort) => void;
};

export function useClassementSettings(): TSettings {
  const [eloStat, changeElo] = useRankingStat<TEloStat>('elo', ELO_STATS, DEFAULT_STATS.elo);
  const [recordStat, changeRecord] = useRankingStat<TRecordStat>(
    'record',
    RECORD_STATS,
    DEFAULT_STATS.record,
  );
  const [paceStat, changePace] = useRankingStat<TPaceStat>('pace', PACE_STATS, DEFAULT_STATS.pace);
  const [sortParam, setSortParam] = useSearchParam('sort');
  const [orderParam, setOrderParam] = useSearchParam('order');
  const changeSort = useCallback(
    (sort: TRankingSort): void => {
      setSortParam(sort.column === 'elo' ? null : sort.column);
      setOrderParam(sort.direction === 'ascending' ? 'asc' : null);
    },
    [setSortParam, setOrderParam],
  );
  return {
    eloStat,
    recordStat,
    paceStat,
    sort: sortOf(sortParam, orderParam),
    changeElo,
    changeRecord,
    changePace,
    changeSort,
  };
}

type TLayout = {
  queueOpen: boolean;
  collapsed: 'ranking' | 'games' | null;
  toggleRanking: () => void;
  toggleGames: () => void;
  openQueue: () => void;
  closeQueue: () => void;
};

export function useClassementLayout(): TLayout {
  const [queueOpen, setQueueOpen] = useState(false);
  const [collapsed, setCollapsed] = useState<'ranking' | 'games' | null>(null);
  const toggleRanking = useCallback(
    (): void => setCollapsed((current) => (current === 'ranking' ? null : 'ranking')),
    [],
  );
  const toggleGames = useCallback(
    (): void => setCollapsed((current) => (current === 'games' ? null : 'games')),
    [],
  );
  const openQueue = useCallback((): void => setQueueOpen(true), []);
  const closeQueue = useCallback((): void => setQueueOpen(false), []);
  return { queueOpen, collapsed, toggleRanking, toggleGames, openQueue, closeQueue };
}
