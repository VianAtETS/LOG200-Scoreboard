// Probabilité de victoire publiée dans model.json (modèle, ou Elo s'il ne le bat pas) : force par équipe
// et avantage du rouge, P(rouge gagne) = σ(sRouge − sNoir + c). Sans couleur, moyenne des deux attributions.

import type { TColor } from '@/types/games.types';
import type { TModel } from '@/types/summary.types';

const sigmoid = (value: number): number => 1 / (1 + Math.exp(-value));

// `null` sans model.json, ou si une des équipes a trop peu de parties.
export function winProbability(
  model: TModel | null | undefined,
  team: string | null,
  opponent: string | null,
  color?: TColor,
): number | null {
  if (!model?.teams || team === null || opponent === null) {
    return null;
  }
  const own = model.teams[team];
  const other = model.teams[opponent];
  if (!own || !other || own.n < model.min_games || other.n < model.min_games) {
    return null;
  }
  const gap = own.s - other.s;
  if (color === 'red') {
    return sigmoid(gap + model.red_advantage);
  }
  if (color === 'black') {
    return sigmoid(gap - model.red_advantage);
  }
  return (sigmoid(gap + model.red_advantage) + sigmoid(gap - model.red_advantage)) / 2;
}

// Entier de 1 à 99 : jamais de certitude affichée.
const MAX_PERCENT = 99;
const PERCENT_SCALE = 100;
export const percent = (probability: number): number =>
  Math.min(MAX_PERCENT, Math.max(1, Math.round(PERCENT_SCALE * probability)));
