import { getLatestRelease, getVersion } from '@/dao/scoreboard.repository';
import { releaseState } from '@/lib/version';
import { useQuery } from '@tanstack/react-query';
import { queryKeys } from './query-keys';

// Version du ladder, comparée à la dernière release GitHub.
export const useGetVersion = () =>
  useQuery({
    queryKey: [queryKeys.version],
    queryFn: async () => {
      const version = await getVersion();
      if (!version) return null;
      const tag = await getLatestRelease().catch(() => undefined);
      return { version, tag, state: releaseState(version, tag) };
    },
    staleTime: Infinity,
  });
