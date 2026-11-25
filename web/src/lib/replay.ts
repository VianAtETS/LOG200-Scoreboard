// Reconstruction des positions d'une partie pour le replay. Les coups sont supposes legaux
// (deja valides par l'arbitre) : on deplace le pion et on detruit la case, sans verification.

export const SIZE = 12;
export const EMPTY = 0;
export const DESTROYED = 1;
export const BLACK = 3;
export const RED = 5;

const MOVE = /^([A-L])(\d{1,2})-([A-L])(\d{1,2})-([A-L])(\d{1,2})$/;

export type TMove = { from: number; to: number; destroyed: number };

export type TStep = {
  cells: number[];
  move: TMove | null;
  text: string | null;
  opening: boolean;
};

// Index lineaire du serveur : ligne 12 en haut (y = 0), colonne A a gauche.
export function squareIndex(col: string, row: number) {
  return (SIZE - row) * SIZE + (col.charCodeAt(0) - 65);
}

export function squareName(index: number) {
  return String.fromCharCode(65 + (index % SIZE)) + (SIZE - Math.floor(index / SIZE));
}

export function parseMove(text: string): TMove {
  const m = MOVE.exec(text);
  if (!m) {
    throw new Error(`coup illisible : ${text}`);
  }
  return {
    from: squareIndex(m[1], Number(m[2])),
    to: squareIndex(m[3], Number(m[4])),
    destroyed: squareIndex(m[5], Number(m[6])),
  };
}

export function initialCells() {
  const cells = new Array<number>(SIZE * SIZE).fill(EMPTY);
  cells[squareIndex('G', 1)] = RED;
  cells[squareIndex('F', 12)] = BLACK;
  return cells;
}

// Positions successives : [0] = plateau initial, puis une par coup (ouverture comprise).
export function positions(game: { opening?: string[]; moves: string[] }) {
  const all = [...(game.opening ?? []), ...game.moves];
  let cells = initialCells();
  const out: TStep[] = [{ cells, move: null, text: null, opening: false }];
  all.forEach((text, i) => {
    const move = parseMove(text);
    cells = [...cells];
    cells[move.to] = cells[move.from];
    cells[move.from] = EMPTY;
    cells[move.destroyed] = DESTROYED;
    out.push({ cells, move, text, opening: i < (game.opening ?? []).length });
  });
  return out;
}
