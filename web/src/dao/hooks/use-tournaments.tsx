import { REFRESH_MS } from '@/dao/constants/api.const';
import {
  getTournament,
  getTournamentIndex,
  getTournamentPreviewText,
  getTournamentText,
} from '@/dao/scoreboard.repository';
import { useQueries, useQuery } from '@tanstack/react-query';
import type { UseQueryResult } from '@tanstack/react-query';
import { queryKeys } from './query-keys';

export const useGetTournamentIndex = (): UseQueryResult<
  Awaited<ReturnType<typeof getTournamentIndex>>
> =>
  useQuery({
    queryKey: [queryKeys.tournaments],
    queryFn: getTournamentIndex,
    refetchInterval: REFRESH_MS,
  });

// Aperçu du prochain tournoi, en texte : `null` s'il n'est pas publié.
export const useGetTournamentPreview = (): UseQueryResult<
  Awaited<ReturnType<typeof getTournamentPreviewText>>
> =>
  useQuery({
    queryKey: [queryKeys.tournament, 'preview'],
    queryFn: getTournamentPreviewText,
    refetchInterval: REFRESH_MS,
  });

// Tableau d'un tournoi, en texte : la page sait s'il a changé.
export const useGetTournamentText = (id: string | null): UseQueryResult<string> =>
  useQuery({
    queryKey: [queryKeys.tournament, id],
    queryFn: async () => {
      if (id === null) {
        throw new Error('Tournament id is required');
      }
      return getTournamentText(id);
    },
    refetchInterval: REFRESH_MS,
    enabled: Boolean(id),
  });

// Tournois terminés : lus une fois, ils ne changent plus.
export const useGetFinishedTournaments = (
  ids: string[],
): UseQueryResult<Awaited<ReturnType<typeof getTournament>> | null>[] =>
  useQueries({
    queries: ids.map((id) => ({
      queryKey: [queryKeys.tournament, 'finished', id],
      queryFn: async () => {
        try {
          return await getTournament(id);
        } catch {
          return null;
        }
      },
      staleTime: Infinity,
    })),
  });

// Tableaux des parties de tournoi en cours (page Direct).
export const useGetTournaments = (
  ids: string[],
): UseQueryResult<Awaited<ReturnType<typeof getTournament>>>[] =>
  useQueries({
    queries: ids.map((id) => ({
      queryKey: [queryKeys.tournament, 'live', id],
      queryFn: async () => getTournament(id),
      refetchInterval: REFRESH_MS,
    })),
  });
