import { BLACK, DESTROYED, RED, SIZE } from '@/lib/replay';
import type { TStep } from '@/lib/replay';
import { cn } from '@/lib/utils';
import type { FC } from 'react';

const FIRST_COLUMN = 'A'.codePointAt(0) ?? 0;
const COLUMNS = Array.from({ length: SIZE }, (_, column) =>
  String.fromCodePoint(FIRST_COLUMN + column),
);

type Props = {
  step: TStep;
  // Les rouges jouent les coups impairs : 1, 3, 5...
  index: number;
  label: string;
  id?: string;
};

// Plateau 12 × 12 d'une position de positions() : cases, pions, et le dernier coup marqué.
const Board: FC<Props> = ({ step, index, label, id }) => {
  const cells = [];
  for (let y = 0; y < SIZE; y++) {
    cells.push(
      <div key={`row-${y}`} className="label">
        {SIZE - y}
      </div>,
    );
    for (let x = 0; x < SIZE; x++) {
      const cellIndex = y * SIZE + x;
      const value = step.cells[cellIndex];
      cells.push(
        <div
          key={cellIndex}
          className={cn(
            'cell',
            value === DESTROYED && 'hole',
            step.move?.from === cellIndex && 'from',
            step.move?.to === cellIndex && 'to',
            step.move?.destroyed === cellIndex && 'destroyed-now',
          )}
        >
          {value === RED && <div className="pawn red" />}
          {value === BLACK && <div className="pawn black" />}
        </div>,
      );
    }
  }

  return (
    <div
      id={id}
      className="board"
      // The board is drawn with child elements, so an img element cannot represent it.
      // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role
      role="img"
      aria-label={label}
      data-mover={index % 2 === 1 ? 'red' : 'black'}
    >
      {cells}
      <div />
      {COLUMNS.map((column) => (
        <div key={`col-${column}`} className="label">
          {column}
        </div>
      ))}
    </div>
  );
};

export { Board };
