import { useGetVersion } from '@/dao/hooks/use-version';
import type { FC } from 'react';

const RELEASE_TITLES = {
  current: (tag?: string) => `À jour : dernière release ${tag}`,
  behind: (tag?: string) => `${tag} disponible`,
  ahead: (tag?: string) => `Plus récente que la dernière release (${tag})`,
};

const VersionBadge: FC = () => {
  const { data } = useGetVersion();

  if (!data) {
    return null;
  }

  const { version, tag, state } = data;
  const title = state === 'unknown' ? 'Version du ladder' : RELEASE_TITLES[state](tag);

  return (
    <span
      className="version"
      title={title}
      data-release={state === 'unknown' ? undefined : state}
      aria-label={state === 'unknown' ? undefined : `Ladder ${version} : ${title}`}
    >
      {version}
    </span>
  );
};

export { VersionBadge };
