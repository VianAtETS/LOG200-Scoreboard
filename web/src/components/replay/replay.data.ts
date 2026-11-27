import type { TDescriptor } from '@/types/game.types';
import type { TColor } from '@/types/games.types';

// Raison de la fin, du point de vue du perdant : liste compacte, puis phrase du replay.
export const REASONS: Record<string, string> = {
  no_moves: 'bloqué',
  timeout: 'temps',
  disconnect: 'déconnexion',
  protocol: 'coups invalides',
  no_show: 'absent',
};

export const ENDINGS: Record<string, (loser: string) => string> = {
  no_moves: (loser) => `${loser} n'a plus de coup`,
  timeout: (loser) => `${loser} a dépassé son temps`,
  disconnect: (loser) => `${loser} s'est déconnecté`,
  protocol: (loser) => `${loser} a joué trop de coups invalides`,
  no_show: (loser) => `${loser} ne s'est pas connecté`,
};

export const PLAY_MS = 500;

// « eq07 gagne, eq12 n'a plus de coup », ou « nulle, répétition » : une fin propre au jeu prend le
// libellé de son descripteur.
export function endText(
  winner: TColor | null | undefined,
  reason: string,
  teams: Record<TColor, string>,
  descriptor: TDescriptor
) {
  const label = descriptor.reasons?.[reason] ?? reason;
  if (!winner) return `nulle, ${label}`;
  const loser = teams[winner === 'red' ? 'black' : 'red'];
  return `${teams[winner]} gagne, ${ENDINGS[reason]?.(loser) ?? label}`;
}
