// Le descripteur du jeu de l'instance et ce que la page en tire. Sans data/game.json (ladder d'avant la
// plateforme multi-jeux, serveur golden de dev), c'est Isolation : le même descripteur que
// crates/isolation-core/src/game.rs.

import type { TColor } from '../types/games.types';
import type { TDescriptor, TPiece } from '../types/game.types';

const ISOLATION_SIZE = 12;

function isolationInitial() {
  const cells = new Array<number>(ISOLATION_SIZE * ISOLATION_SIZE).fill(0);
  cells[5] = 3; // F12, noir
  cells[138] = 5; // G1, rouge
  return cells;
}

export const ISOLATION: TDescriptor = {
  game: 'isolation',
  version: 1,
  name: 'Isolation',
  rows: ISOLATION_SIZE,
  cols: ISOLATION_SIZE,
  sides: { red: { label: 'Rouge' }, black: { label: 'Noir' } },
  initial: isolationInitial(),
  pieces: [
    { code: 0, label: 'case vide', style: 'empty' },
    { code: 1, label: 'case détruite', style: 'hole' },
    { code: 3, label: 'pion noir', style: 'pawn', side: 'black' },
    { code: 5, label: 'pion rouge', style: 'pawn', side: 'red' },
  ],
  reasons: { no_moves: 'plus aucun coup' },
};

// Un descripteur publié est utilisable s'il est cohérent ; sinon, Isolation.
export function usable(value: unknown): value is TDescriptor {
  const d = value as TDescriptor | null;
  return (
    !!d &&
    typeof d.game === 'string' &&
    Number.isInteger(d.rows) &&
    Number.isInteger(d.cols) &&
    d.rows > 0 &&
    d.cols > 0 &&
    Array.isArray(d.initial) &&
    d.initial.length === d.rows * d.cols &&
    Array.isArray(d.pieces) &&
    !!d.sides?.red?.label &&
    !!d.sides?.black?.label
  );
}

export const pieceOf = (d: TDescriptor, code: number): TPiece | undefined => d.pieces.find((p) => p.code === code);

export const sideLabel = (d: TDescriptor, color: TColor) => d.sides[color].label;

// Colonnes A, B... de gauche à droite ; lignes numérotées depuis le bas, comme la notation du serveur.
export const columnName = (x: number) => String.fromCharCode(65 + x);
export const rowName = (d: TDescriptor, y: number) => String(d.rows - y);
export const cellName = (d: TDescriptor, index: number) =>
  columnName(index % d.cols) + rowName(d, Math.floor(index / d.cols));
