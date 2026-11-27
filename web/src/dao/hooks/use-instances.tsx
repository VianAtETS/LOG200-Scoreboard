import { getInstanceCard, getInstances } from '@/dao/scoreboard.repository';
import { useQuery } from '@tanstack/react-query';
import { queryKeys } from './query-keys';

// Les instances du serveur à plusieurs jeux dont `root` est la racine ; `null` pour un serveur d'une
// seule instance (pas d'instances.json).
export const useInstances = (root: string) =>
  useQuery({
    queryKey: [queryKeys.instances, root],
    queryFn: () => getInstances(root),
    staleTime: 60_000,
  });

export const useInstanceCard = (data: string) =>
  useQuery({
    queryKey: [queryKeys.instanceCard, data],
    queryFn: () => getInstanceCard(data),
    staleTime: 60_000,
  });
