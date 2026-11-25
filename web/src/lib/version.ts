export type TReleaseState = 'current' | 'behind' | 'ahead' | 'unknown';

const parse = (version: string | null | undefined) => {
  const parts = (version ?? '').replace(/^v/, '').split('.');
  return parts.length === 3 && parts.every((p) => /^\d+$/.test(p)) ? parts.map(Number) : null;
};

export function releaseState(
  ladder: string | null | undefined,
  latestTag: string | null | undefined,
): TReleaseState {
  const [mine, latest] = [parse(ladder), parse(latestTag)];
  if (!mine || !latest) return 'unknown';
  const differs = mine.findIndex((part, i) => part !== latest[i]);
  if (differs < 0) return 'current';
  return mine[differs] < latest[differs] ? 'behind' : 'ahead';
}
