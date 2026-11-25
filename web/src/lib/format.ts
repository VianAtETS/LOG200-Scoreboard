export const timeFormat = new Intl.DateTimeFormat('fr-CA', { timeStyle: 'short' });
export const dayFormat = new Intl.DateTimeFormat('fr-CA', { day: 'numeric', month: 'short' });
export const fullFormat = new Intl.DateTimeFormat('fr-CA', {
  dateStyle: 'long',
  timeStyle: 'short',
});

export const signed = (value: number): string =>
  `${value > 0 ? '+' : '−'}${Math.abs(value).toLocaleString('fr-CA')}`;
const TENTHS = 10;
const MINUTE_MS = 60_000;
const MINUTES_PER_HOUR = 60;
const HOURS_PER_DAY = 24;

export const roundDelta = (delta: number): number =>
  Math.round(delta) || Math.round(delta * TENTHS) / TENTHS;

const sameDay = (first: Date, second: Date): boolean =>
  first.toDateString() === second.toDateString();
export function shortWhen(ts: string): string {
  const date = new Date(ts);
  return sameDay(date, new Date()) ? timeFormat.format(date) : dayFormat.format(date);
}

export function ago(ts: string): string {
  const minutes = Math.floor((Date.now() - Date.parse(ts)) / MINUTE_MS);
  if (minutes < 1) {
    return "à l'instant";
  }
  if (minutes < MINUTES_PER_HOUR) {
    return `il y a ${minutes} min`;
  }
  if (minutes < HOURS_PER_DAY * MINUTES_PER_HOUR) {
    return `il y a ${Math.floor(minutes / MINUTES_PER_HOUR)} h`;
  }
  return dayFormat.format(new Date(ts));
}

export const ordinal = (rank: number): string => (rank === 1 ? '1re' : `${rank}e`);

// "v3" pour le build `build` de `team`, vide sans build.
export function versionOf(builds: Map<string, string[]>, team: string, build?: string): string {
  const number =
    build === undefined || build === '' ? 0 : (builds.get(team)?.indexOf(build) ?? -1) + 1;
  return number === 0 ? '' : `v${number}`;
}
