// Reconstruction des positions d'une partie pour le replay, sans connaître les règles : la position
// initiale du descripteur, puis les effets de chaque demi-coup (`changes`, rendus par le ladder). Sans
// effets (partie pas encore rejouée par le ladder, direct d'un ancien arbitre), une partie d'Isolation
// se rejoue encore à partir de ses coups, qui disent tout : départ, arrivée, case détruite.

import type { TChange, TDescriptor } from '../types/game.types';
import { ISOLATION } from './game.ts';

// Isolation : plateau, codes des cases et notation (crates/isolation-core).
export const SIZE = 12;
export const EMPTY = 0;
export const DESTROYED = 1;
export const BLACK = 3;
export const RED = 5;

const MOVE = /^([A-L])(\d{1,2})-([A-L])(\d{1,2})-([A-L])(\d{1,2})$/;

export type TMove = { from: number; to: number; destroyed: number };

export type TStep = {
  cells: number[];
  // Coup d'Isolation lu, pour le décrire ; `null` pour la position initiale et les autres jeux.
  move: TMove | null;
  text: string | null;
  opening: boolean;
  // Effets du demi-coup qui mène à cette position (vide pour la position initiale).
  changes: TChange[];
};

// Index linéaire du serveur : ligne 12 en haut (y = 0), colonne A à gauche.
export function squareIndex(col: string, row: number) {
  return (SIZE - row) * SIZE + (col.charCodeAt(0) - 65);
}

export function squareName(index: number) {
  return String.fromCharCode(65 + (index % SIZE)) + (SIZE - Math.floor(index / SIZE));
}

export function parseMove(text: string): TMove {
  const m = MOVE.exec(text);
  if (!m) throw new Error(`coup illisible : ${text}`);
  return {
    from: squareIndex(m[1], +m[2]),
    to: squareIndex(m[3], +m[4]),
    destroyed: squareIndex(m[5], +m[6]),
  };
}

const tryParse = (text: string) => (MOVE.test(text) ? parseMove(text) : null);

export function initialCells() {
  return ISOLATION.initial.slice();
}

// Les effets d'un coup d'Isolation, comme les rend crates/isolation-core/src/game.rs.
function isolationChanges(move: TMove, cells: number[]): TChange[] {
  return [
    { cell: move.from, code: EMPTY, role: 'from' },
    { cell: move.to, code: cells[move.from], role: 'to' },
    { cell: move.destroyed, code: DESTROYED, role: 'effect' },
  ];
}

type TPlayed = { opening?: string[]; moves: string[]; changes?: TChange[][] };

// Positions successives : [0] = plateau initial, puis une par coup (ouverture comprise). S'arrête au
// premier coup qu'elle ne sait pas rejouer (ni effets ni notation d'Isolation).
export function positions(game: TPlayed, descriptor: TDescriptor = ISOLATION): TStep[] {
  const opening = game.opening ?? [];
  const all = [...opening, ...game.moves];
  const isolation = descriptor.game === 'isolation';
  let cells = descriptor.initial.slice();
  const out: TStep[] = [{ cells, move: null, text: null, opening: false, changes: [] }];
  for (const [i, text] of all.entries()) {
    const move = isolation ? tryParse(text) : null;
    const changes = game.changes?.[i] ?? (move ? isolationChanges(move, cells) : null);
    if (!changes) break;
    cells = cells.slice();
    for (const c of changes) cells[c.cell] = c.code;
    out.push({ cells, move, text, opening: i < opening.length, changes });
  }
  return out;
}
