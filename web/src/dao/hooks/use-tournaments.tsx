import { REFRESH_MS } from '@/dao/constants/api.const';
import {
  getTournament,
  getTournamentIndex,
  getTournamentPreviewText,
  getTournamentText,
} from '@/dao/scoreboard.repository';
import { useQueries, useQuery } from '@tanstack/react-query';
import { queryKeys } from './query-keys';

export const useGetTournamentIndex = () =>
  useQuery({
    queryKey: [queryKeys.tournaments],
    queryFn: getTournamentIndex,
    refetchInterval: REFRESH_MS,
  });

// Aperçu du prochain tournoi, en texte : `null` s'il n'est pas publié.
export const useGetTournamentPreview = () =>
  useQuery({
    queryKey: [queryKeys.tournament, 'preview'],
    queryFn: getTournamentPreviewText,
    refetchInterval: REFRESH_MS,
  });

// Tableau d'un tournoi, en texte : la page sait s'il a changé.
export const useGetTournamentText = (id: string | null) =>
  useQuery({
    queryKey: [queryKeys.tournament, id],
    queryFn: async () => getTournamentText(id!),
    refetchInterval: REFRESH_MS,
    enabled: Boolean(id),
  });

// Tournois terminés : lus une fois, ils ne changent plus.
export const useGetFinishedTournaments = (ids: string[]) =>
  useQueries({
    queries: ids.map((id) => ({
      queryKey: [queryKeys.tournament, 'finished', id],
      queryFn: async () => getTournament(id).catch(() => null),
      staleTime: Infinity,
    })),
  });

// Tableaux des parties de tournoi en cours (page Direct).
export const useGetTournaments = (ids: string[]) =>
  useQueries({
    queries: ids.map((id) => ({
      queryKey: [queryKeys.tournament, 'live', id],
      queryFn: async () => getTournament(id),
      refetchInterval: REFRESH_MS,
    })),
  });
