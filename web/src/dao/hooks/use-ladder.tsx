import { REFRESH_MS } from '@/dao/constants/api.const';
import { getHealth, getModel, getQueue, getRunning, getSummary } from '@/dao/scoreboard.repository';
import { useQuery } from '@tanstack/react-query';
import type { DefinedUseQueryResult, UseQueryResult } from '@tanstack/react-query';
import { queryKeys } from './query-keys';

export const useGetSummary = (): UseQueryResult<Awaited<ReturnType<typeof getSummary>>> =>
  useQuery({
    queryKey: [queryKeys.summary],
    queryFn: getSummary,
    refetchInterval: REFRESH_MS,
  });

// Parties en cours, d'après live.json.
export const useGetRunning = (): DefinedUseQueryResult<Awaited<ReturnType<typeof getRunning>>> =>
  useQuery({
    queryKey: [queryKeys.running],
    queryFn: async () => {
      try {
        return await getRunning();
      } catch {
        return [];
      }
    },
    refetchInterval: REFRESH_MS,
    initialData: [],
  });

// Modèle de probabilité de model.json, `null` s'il n'est pas publié.
export const useGetModel = (): UseQueryResult<Awaited<ReturnType<typeof getModel>>> =>
  useQuery({
    queryKey: [queryKeys.model],
    queryFn: getModel,
    refetchInterval: REFRESH_MS,
  });

// Relue seulement quand la file est ouverte.
export const useGetQueue = (
  enabled: boolean,
): UseQueryResult<Awaited<ReturnType<typeof getQueue>>> =>
  useQuery({
    queryKey: [queryKeys.queue],
    queryFn: getQueue,
    refetchInterval: REFRESH_MS,
    enabled,
  });

export const useGetHealth = (): UseQueryResult<Awaited<ReturnType<typeof getHealth>>> =>
  useQuery({
    queryKey: [queryKeys.health],
    queryFn: getHealth,
    staleTime: Infinity,
  });
