import { getDescriptor } from '@/dao/scoreboard.repository';
import { ISOLATION } from '@/lib/game';
import { useQuery } from '@tanstack/react-query';
import { queryKeys } from './query-keys';

// Le jeu de l'instance, lu une fois. Isolation en attendant la réponse : le jeu de toujours, et celui
// d'un ladder sans data/game.json.
export const useDescriptor = () => {
  const { data } = useQuery({
    queryKey: [queryKeys.descriptor],
    queryFn: getDescriptor,
    staleTime: Infinity,
  });
  return data ?? ISOLATION;
};
