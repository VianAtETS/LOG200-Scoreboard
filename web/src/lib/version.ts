const VERSION_COMPONENTS = 3;
export type TReleaseState = 'current' | 'behind' | 'ahead' | 'unknown';

const parse = (version: string | null | undefined): number[] | null => {
  const parts = (version ?? '').replace(/^v/u, '').split('.');
  return parts.length === VERSION_COMPONENTS && parts.every((part) => /^\d+$/u.test(part))
    ? parts.map(Number)
    : null;
};

export function releaseState(
  ladder: string | null | undefined,
  latestTag: string | null | undefined,
): TReleaseState {
  const [mine, latest] = [parse(ladder), parse(latestTag)];
  if (!mine || !latest) {
    return 'unknown';
  }
  const differs = mine.findIndex((part, i) => part !== latest[i]);
  if (differs === -1) {
    return 'current';
  }
  return (mine[differs] ?? 0) < (latest[differs] ?? 0) ? 'behind' : 'ahead';
}
