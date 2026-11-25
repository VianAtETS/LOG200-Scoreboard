import { tournamentDuration } from '@/lib/estimate';
import type { TBracketSide, TTournament } from '@/types/tournament.types';

export const SIDES: [TBracketSide, string][] = [
  ['winners', 'Tableau principal'],
  ['losers', 'Repêchage'],
];
export const MATCH_STATUS: Record<string, string> = { running: 'en cours', suspended: 'suspendu' };
export const PREVIEW = 'apercu';
// Tournois terminés qui calibrent la durée prévue.
export const CALIBRATION = 5;

export const dateFormat = new Intl.DateTimeFormat('fr-CA', {
  weekday: 'long',
  day: 'numeric',
  month: 'long',
});
export const shortDate = new Intl.DateTimeFormat('fr-CA', {
  day: 'numeric',
  month: 'short',
  year: 'numeric',
});

const MILLISECONDS_PER_MINUTE = 60_000;
const MINUTES_PER_HOUR = 60;

export function span(ms: number): string {
  const minutes = Math.round(ms / MILLISECONDS_PER_MINUTE);
  const h = Math.floor(minutes / MINUTES_PER_HOUR);
  return h ? `${h} h ${String(minutes % MINUTES_PER_HOUR).padStart(2, '0')}` : `${minutes} min`;
}

export function predicted(view: TTournament, factor: number, fromStart = true): number | null {
  const ms = tournamentDuration(view, fromStart);
  return ms === null ? null : ms * factor;
}

export function roundTitle(side: TBracketSide, round?: number, last?: number): string {
  if (side === 'final') {
    return 'Grande finale';
  }
  if (round === last) {
    return side === 'winners' ? 'Finale du principal' : 'Finale du repêchage';
  }
  return side === 'winners' ? `Tour ${round}` : `Repêchage ${round}`;
}
