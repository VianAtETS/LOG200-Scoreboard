// Probabilité de victoire publiée dans model.json (modèle, ou Elo s'il ne le bat pas) : force par équipe
// et avantage du rouge, P(rouge gagne) = σ(sRouge − sNoir + c). Sans couleur, moyenne des deux attributions.

import type { TColor } from '../types/games.types';
import type { TModel } from '../types/summary.types';

const sigmoid = (x: number) => 1 / (1 + Math.exp(-x));

// `null` sans model.json, ou si une des équipes a trop peu de parties.
export function winProbability(
  model: TModel | null | undefined,
  team: string | null,
  opponent: string | null,
  color?: TColor,
) {
  if (!model?.teams || team === null || opponent === null) {
    return null;
  }
  const a = model.teams[team];
  const b = model.teams[opponent];
  if (!a || !b || a.n < model.min_games || b.n < model.min_games) {
    return null;
  }
  const gap = a.s - b.s;
  if (color === 'red') {
    return sigmoid(gap + model.red_advantage);
  }
  if (color === 'black') {
    return sigmoid(gap - model.red_advantage);
  }
  return (sigmoid(gap + model.red_advantage) + sigmoid(gap - model.red_advantage)) / 2;
}

// Entier de 1 à 99 : jamais de certitude affichée.
export const percent = (p: number) => Math.min(99, Math.max(1, Math.round(100 * p)));
