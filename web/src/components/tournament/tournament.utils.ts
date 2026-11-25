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

export function span(ms: number) {
  const minutes = Math.round(ms / 60_000);
  const h = Math.floor(minutes / 60);
  return h ? `${h} h ${String(minutes % 60).padStart(2, '0')}` : `${minutes} min`;
}

export function predicted(view: TTournament, factor: number, fromStart = true) {
  const ms = tournamentDuration(view, fromStart);
  return ms === null ? null : ms * factor;
}

export function roundTitle(side: TBracketSide, round?: number, last?: number) {
  if (side === 'final') {
    return 'Grande finale';
  }
  if (round === last) {
    return side === 'winners' ? 'Finale du principal' : 'Finale du repêchage';
  }
  return side === 'winners' ? `Tour ${round}` : `Repêchage ${round}`;
}
