import { GLOBAL } from '@/lib/constants';
import { BETWEEN, gamesOf, isRemote, ratingOf } from '@/lib/elo';
import { useGetTeamRows } from '@/dao/hooks/use-games';
import { useGetSummary } from '@/dao/hooks/use-ladder';
import { usePulse } from '@/hooks/use-pulse';
import { useSearchParam } from '@/hooks/use-search-param';
import { archiveOptions, archivedRows, withArchivedRows } from '@/lib/archives';
import type { TArchiveOption, TRankingRow } from '@/lib/archives';
import { versionOf } from '@/lib/format';
import type { TGameRow, TRatingChange, TSide } from '@/types/games.types';
import type { TScope, TSummary, TTeamRating } from '@/types/summary.types';
// oxlint-disable-next-line import/max-dependencies -- Ranking data combines query results, archive selection and derived views.
import { useCallback, useMemo } from 'react';

const inGameScope = (game: TGameRow, session: string, verified: boolean): boolean =>
  (!verified || !isRemote(game)) && (session !== GLOBAL || !game.tournament);
type TArchive = {
  rankingRows: TRankingRow[];
  archiveChoices: TArchiveOption[];
  wantedArchives: string[];
  archiveError: Error | null;
  chooseArchives: (choices: string[]) => void;
};

function useRankingArchives(
  summary: TSummary | undefined,
  { scope, session, verified }: { scope: TScope | undefined; session: string; verified: boolean },
): TArchive {
  // Versions archivées : seulement avec un ladder lancé avec --archives.
  const [archivesParam, setArchivesParam] = useSearchParam('archives');
  const chooseArchives = useCallback(
    (next: string[]): void => setArchivesParam(next.join(',') || null),
    [setArchivesParam],
  );
  const archiveChoices = useMemo(
    () => (summary?.archives ? archiveOptions(summary) : []),
    [summary],
  );
  const wantedArchives = useMemo(
    () => (archivesParam ?? '').split(',').filter(Boolean),
    [archivesParam],
  );
  const pickedArchives = useMemo(() => {
    const wanted = new Set(wantedArchives);
    return archiveChoices.filter((option) => wanted.has(option.key));
  }, [archiveChoices, wantedArchives]);
  const archiveTeamsKey = [...new Set(pickedArchives.map((option) => option.team))].join(',');
  const archiveTeams = useMemo(() => archiveTeamsKey.split(',').filter(Boolean), [archiveTeamsKey]);
  const { data: archiveFiles, error: archiveError } = useGetTeamRows(archiveTeams, 'archives');
  const rankingRows = useMemo(() => {
    const games = new Map(
      [...(archiveFiles?.values() ?? [])].flat().map((game) => [game.id, game]),
    );
    return withArchivedRows(
      scope?.ratings ?? [],
      archivedRows([...games.values()], pickedArchives, { session, verified }),
    );
  }, [archiveFiles, pickedArchives, scope, session, verified]);
  return { rankingRows, archiveChoices, wantedArchives, archiveError, chooseArchives };
}

type TInput = {
  sessionChoice: string | null;
  verifiedParam: string | null;
  match: string;
  chosen: string[];
};

type TClassement = {
  summary: TSummary | undefined;
  error: Error | null;
  dataUpdatedAt: number;
  session: string;
  scope: TScope | undefined;
  ratings: TTeamRating[];
  version: (team: string, build?: string) => string;
  rating: (side: TSide) => TRatingChange;
  teams: string[];
  teamsKey: string;
  list: TGameRow[];
  duelGames: TGameRow[];
  pulses: number;
};

export function useClassementData({
  sessionChoice,
  verifiedParam,
  match,
  chosen,
}: TInput): TClassement & TArchive {
  const { data: summary, error, dataUpdatedAt } = useGetSummary();
  // Sans ?session=, la session la plus récente.
  const sessions = summary?.sessions ?? [];
  const wanted = sessionChoice ?? sessions[0] ?? GLOBAL;
  const session = sessions.includes(wanted) ? wanted : GLOBAL;
  const key = session === GLOBAL ? 'all' : 'session';
  const verified = Boolean(summary?.verified) && verifiedParam === '1';
  const scope = (verified ? summary?.verified : summary?.scopes)?.[session];
  const ratings = scope?.ratings ?? [];
  const builds = useMemo(() => new Map(Object.entries(summary?.builds ?? {})), [summary]);
  const version = useCallback(
    (team: string, build?: string) => versionOf(builds, team, build),
    [builds],
  );
  const rating = useCallback((side: TSide) => ratingOf(side, key, verified), [key, verified]);
  const known = useMemo(
    () => new Set(summary?.scopes[GLOBAL]?.ratings.map((row) => row.team)),
    [summary],
  );
  const teamsKey = summary ? chosen.filter((team) => known.has(team)).join(',') : '';
  const teams = useMemo(() => teamsKey.split(',').filter(Boolean), [teamsKey]);
  const { data: rows, isPlaceholderData } = useGetTeamRows(teams);
  const archive = useRankingArchives(summary, { scope, session, verified });
  const list = useMemo(() => {
    if (!scope) {
      return [];
    }
    if (teams.length === 0) {
      return scope.recent;
    }
    return gamesOf(rows ?? new Map<string, TGameRow[]>(), teams, { mode: match, session }).filter(
      (game) => inGameScope(game, session, verified),
    );
  }, [scope, teams, rows, match, session, verified]);
  const duelGames = useMemo(
    () =>
      teams.length === 2
        ? gamesOf(rows ?? new Map<string, TGameRow[]>(), teams, { mode: BETWEEN, session }).filter(
            (game) => inGameScope(game, session, verified),
          )
        : [],
    [teams, rows, session, verified],
  );
  // Le point du direct pulse quand des parties arrivent, pas au chargement ni à un changement d'équipes.
  const pulses = usePulse(summary) + usePulse(isPlaceholderData ? undefined : rows, teamsKey);
  return {
    summary,
    error,
    dataUpdatedAt,
    session,
    scope,
    ratings,
    version,
    rating,
    teams,
    teamsKey,
    list,
    duelGames,
    pulses,
    ...archive,
  };
}
