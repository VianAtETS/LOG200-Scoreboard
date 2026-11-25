import { getLatestRelease, getVersion } from '@/dao/scoreboard.repository';
import { releaseState } from '@/lib/version';
import { useQuery } from '@tanstack/react-query';
import type { UseQueryResult } from '@tanstack/react-query';
import { queryKeys } from './query-keys';

// Version du ladder, comparée à la dernière release GitHub.
export const useGetVersion = (): UseQueryResult<{
  version: string;
  tag: string | undefined;
  state: ReturnType<typeof releaseState>;
} | null> =>
  useQuery({
    queryKey: [queryKeys.version],
    queryFn: async () => {
      const version = await getVersion();
      if (version === null || version === '') {
        return null;
      }
      let tag: string | undefined;
      try {
        tag = await getLatestRelease();
      } catch {
        // The installed version remains useful if the release lookup fails.
      }
      return { version, tag, state: releaseState(version, tag) };
    },
    staleTime: Infinity,
  });
