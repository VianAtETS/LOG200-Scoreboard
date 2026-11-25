export const timeFormat = new Intl.DateTimeFormat('fr-CA', { timeStyle: 'short' });
export const dayFormat = new Intl.DateTimeFormat('fr-CA', { day: 'numeric', month: 'short' });
export const fullFormat = new Intl.DateTimeFormat('fr-CA', {
  dateStyle: 'long',
  timeStyle: 'short',
});

export const signed = (n: number) => `${n > 0 ? '+' : '−'}${Math.abs(n).toLocaleString('fr-CA')}`;
export const roundDelta = (d: number) => Math.round(d) || Math.round(d * 10) / 10;

const sameDay = (a: Date, b: Date) => a.toDateString() === b.toDateString();

export function shortWhen(ts: string) {
  const d = new Date(ts);
  return sameDay(d, new Date()) ? timeFormat.format(d) : dayFormat.format(d);
}

export function ago(ts: string) {
  const minutes = Math.floor((Date.now() - Date.parse(ts)) / 60000);
  if (minutes < 1) return "à l'instant";
  if (minutes < 60) return `il y a ${minutes} min`;
  if (minutes < 24 * 60) return `il y a ${Math.floor(minutes / 60)} h`;
  return dayFormat.format(new Date(ts));
}

export const ordinal = (rank: number) => (rank === 1 ? '1re' : `${rank}e`);

// "v3" pour le build `build` de `team`, vide sans build.
export function versionOf(builds: Map<string, string[]>, team: string, build?: string) {
  const n = build ? (builds.get(team)?.indexOf(build) ?? -1) + 1 : 0;
  return n ? `v${n}` : '';
}
