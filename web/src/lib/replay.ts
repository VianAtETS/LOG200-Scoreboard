// Reconstruction des positions d'une partie pour le replay. Les coups sont supposes legaux
// (deja valides par l'arbitre) : on deplace le pion et on detruit la case, sans verification.

export const SIZE = 12;
export const EMPTY = 0;
export const DESTROYED = 1;
export const BLACK = 3;
export const RED = 5;

const MOVE =
  /^(?<fromCol>[A-L])(?<fromRow>\d{1,2})-(?<toCol>[A-L])(?<toRow>\d{1,2})-(?<destroyCol>[A-L])(?<destroyRow>\d{1,2})$/u;
const FIRST_COLUMN = 65;

export type TMove = { from: number; to: number; destroyed: number };

export type TStep = {
  cells: number[];
  move: TMove | null;
  text: string | null;
  opening: boolean;
};

// Index lineaire du serveur : ligne 12 en haut (y = 0), colonne A a gauche.
export function squareIndex(col: string, row: number): number {
  return (SIZE - row) * SIZE + ((col.codePointAt(0) ?? Number.NaN) - FIRST_COLUMN);
}

export function squareName(index: number): string {
  const code = FIRST_COLUMN + (index % SIZE);
  return (
    String.fromCodePoint(Number.isNaN(code) ? 0 : Math.trunc(code)) +
    (SIZE - Math.floor(index / SIZE))
  );
}

export function parseMove(text: string): TMove {
  const match = MOVE.exec(text);
  const groups = match?.groups;
  if (
    groups?.['fromCol'] === undefined ||
    groups['toCol'] === undefined ||
    groups['destroyCol'] === undefined
  ) {
    throw new Error(`coup illisible : ${text}`);
  }
  return {
    from: squareIndex(groups['fromCol'], Number(groups['fromRow'])),
    to: squareIndex(groups['toCol'], Number(groups['toRow'])),
    destroyed: squareIndex(groups['destroyCol'], Number(groups['destroyRow'])),
  };
}

export function initialCells(): number[] {
  const cells = Array.from<number>({ length: SIZE * SIZE }).fill(EMPTY);
  cells[squareIndex('G', 1)] = RED;
  cells[squareIndex('F', SIZE)] = BLACK;
  return cells;
}

const copyCells = (cells: number[]): number[] => [...cells];
// Positions successives : [0] = plateau initial, puis une par coup (ouverture comprise).
export function positions(game: { opening?: string[]; moves: string[] }): TStep[] {
  const all = [...(game.opening ?? []), ...game.moves];
  let cells = initialCells();
  const out: TStep[] = [{ cells, move: null, text: null, opening: false }];
  for (const [i, text] of all.entries()) {
    const move = parseMove(text);
    cells = copyCells(cells);
    Object.assign(cells, { [move.to]: cells[move.from] });
    cells[move.from] = EMPTY;
    cells[move.destroyed] = DESTROYED;
    out.push({ cells, move, text, opening: i < (game.opening ?? []).length });
  }
  return out;
}
