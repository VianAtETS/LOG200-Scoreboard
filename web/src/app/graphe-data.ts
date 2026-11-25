import { colorOf } from '@/components/bump/bump.utils';
import { useGetTimeline } from '@/dao/hooks/use-games';
import { useGetSummary } from '@/dao/hooks/use-ladder';
import { usePulse } from '@/hooks/use-pulse';
import { rankSeries, topTeams } from '@/lib/bump';
import { timeFormat, versionOf } from '@/lib/format';
import { GLOBAL } from '@/lib/constants';
import type { TTeamRating } from '@/types/summary.types';
import { useCallback, useMemo } from 'react';

type TStatus = { dot: '' | 'on' | 'error'; text: string };

type TGraphData = ReturnType<typeof rankSeries> & {
  session: string;
  ratings: TTeamRating[];
  selected: string[];
  palette: Map<string, string>;
  version: (team: string, build?: string) => string;
  pulses: number;
  sessions: string[] | undefined;
  status: TStatus & { hasVerified: boolean };
};

type TGraphParams = {
  sessionChoice: string | null;
  verifiedParam: string | null;
  teamsParam: string | null;
  duration: number;
};

function graphStatus(
  failure: Error | null,
  ready: boolean,
  count: number,
  updatedAt: Date,
): TStatus {
  if (failure) {
    return { dot: 'error', text: `Données indisponibles : ${failure.message}` };
  }
  if (!ready) {
    return { dot: '', text: 'Chargement des parties…' };
  }
  return {
    dot: 'on',
    text: `${count} parties dans la fenêtre, à jour à ${timeFormat.format(updatedAt)}`,
  };
}

export function useGraphData({
  sessionChoice,
  verifiedParam,
  teamsParam,
  duration,
}: TGraphParams): TGraphData {
  const { data: summary, error, dataUpdatedAt } = useGetSummary();
  const {
    data: timeline,
    error: timelineError,
    dataUpdatedAt: timelineUpdatedAt,
  } = useGetTimeline();
  // Sans ?session=, la session la plus récente.
  const sessions = summary?.sessions ?? [];
  const wanted = sessionChoice ?? sessions[0] ?? GLOBAL;
  const session = sessions.includes(wanted) ? wanted : GLOBAL;
  const key = session === GLOBAL ? 'all' : 'session';
  const verified = Boolean(summary?.verified) && verifiedParam === '1';
  const scope = (verified ? summary?.verified : summary?.scopes)?.[session];
  const ratings = useMemo(() => scope?.ratings ?? [], [scope]);
  const builds = useMemo(() => new Map(Object.entries(summary?.builds ?? {})), [summary]);
  const version = useCallback(
    (team: string, build?: string) => versionOf(builds, team, build),
    [builds],
  );
  const selected = useMemo(() => {
    if (teamsParam === null) {
      return topTeams(ratings);
    }
    const known = new Set(ratings.map((row) => row.team));
    return teamsParam.split(',').filter((team) => team !== '' && known.has(team));
  }, [teamsParam, ratings]);
  const cutoff = dataUpdatedAt - duration;
  const { columns, series } = useMemo(() => {
    const rows =
      session === GLOBAL ? (timeline ?? []).filter((row) => !row.tournament) : (timeline ?? []);
    return rankSeries(rows, {
      key,
      session,
      selected,
      cutoff,
      verified,
    });
  }, [timeline, key, session, selected, cutoff, verified]);
  // Un bot reste gris et ne prend pas de couleur.
  const palette = useMemo(() => {
    const bots = new Set<string>();
    for (const rating of ratings) {
      if (rating.build === undefined || rating.build === '') {
        bots.add(rating.team);
      }
    }
    const colors = new Map<string, string>();
    for (const team of selected) {
      if (!bots.has(team)) {
        colors.set(team, colorOf(team, selected));
      }
    }
    return colors;
  }, [ratings, selected]);
  const pulses = usePulse(summary) + usePulse(timeline);
  const failure = error ?? timelineError;
  const ready = summary !== undefined && timeline !== undefined;
  const updatedAt = new Date(Math.max(dataUpdatedAt, timelineUpdatedAt));
  const status = graphStatus(failure, ready, columns.length, updatedAt);
  return {
    session,
    ratings,
    selected,
    columns,
    series,
    palette,
    version,
    pulses,
    sessions: summary?.sessions,
    status: { ...status, hasVerified: summary?.verified !== undefined },
  };
}
