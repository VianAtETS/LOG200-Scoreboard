// Le jeu d'une instance, tel que l'arbitre le décrit (`describe`, publié dans data/game.json) :
// docs/spec-multi-jeux.md. La page dessine n'importe quel jeu sur grille sans en connaître les règles.

import type { TColor } from './games.types';

export type TPiece = {
  code: number;
  label: string;
  // Dessin : empty, hole, pawn, king... ; un style inconnu est une pièce générique de son camp.
  style: string;
  side?: TColor;
};

export type TSpecial = { cell: number; style: string };

export type TDescriptor = {
  game: string;
  version: number;
  name: string;
  rows: number;
  cols: number;
  sides: Record<TColor, { label: string }>;
  // rows × cols codes, ligne du haut d'abord.
  initial: number[];
  pieces: TPiece[];
  special?: TSpecial[];
  // Demi-coups au plus d'une partie, si les règles en fixent.
  max_plies?: number;
  // Fins propres au jeu et leur libellé.
  reasons?: Record<string, string>;
};

// Un effet d'un demi-coup : la case (index du protocole), sa nouvelle valeur, et son rôle pour marquer
// le dernier coup.
export type TRole = 'from' | 'to' | 'effect';
export type TChange = { cell: number; code: number; role: TRole };
