import { REFRESH_MS } from '@/dao/constants/api.const';
import { getHealth, getModel, getQueue, getRunning, getSummary } from '@/dao/scoreboard.repository';
import { useQuery } from '@tanstack/react-query';
import { queryKeys } from './query-keys';

export const useGetSummary = () =>
  useQuery({
    queryKey: [queryKeys.summary],
    queryFn: getSummary,
    refetchInterval: REFRESH_MS,
  });

// Parties en cours, d'après live.json.
export const useGetRunning = () =>
  useQuery({
    queryKey: [queryKeys.running],
    queryFn: async () => getRunning().catch(() => []),
    refetchInterval: REFRESH_MS,
    initialData: [],
  });

// Modèle de probabilité de model.json, `null` s'il n'est pas publié.
export const useGetModel = () =>
  useQuery({
    queryKey: [queryKeys.model],
    queryFn: getModel,
    refetchInterval: REFRESH_MS,
  });

// Relue seulement quand la file est ouverte.
export const useGetQueue = (enabled: boolean) =>
  useQuery({
    queryKey: [queryKeys.queue],
    queryFn: getQueue,
    refetchInterval: REFRESH_MS,
    enabled,
  });

export const useGetHealth = () =>
  useQuery({
    queryKey: [queryKeys.health],
    queryFn: getHealth,
    staleTime: Infinity,
  });
