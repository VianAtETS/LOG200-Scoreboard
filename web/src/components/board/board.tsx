import { BLACK, DESTROYED, RED, SIZE } from '@/lib/replay';
import type { TStep } from '@/lib/replay';
import { cn } from '@/lib/utils';
import type { FC } from 'react';

const COLUMNS = Array.from({ length: SIZE }, (_, x) => String.fromCharCode(65 + x));

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
      const i = y * SIZE + x;
      const v = step.cells[i];
      cells.push(
        <div
          key={i}
          className={cn(
            'cell',
            v === DESTROYED && 'hole',
            step.move?.from === i && 'from',
            step.move?.to === i && 'to',
            step.move?.destroyed === i && 'destroyed-now',
          )}
        >
          {v === RED && <div className="pawn red" />}
          {v === BLACK && <div className="pawn black" />}
        </div>,
      );
    }
  }

  return (
    <div
      id={id}
      className="board"
      role="img"
      aria-label={label}
      data-mover={index % 2 === 1 ? 'red' : 'black'}
    >
      {cells}
      <div />
      {COLUMNS.map((c) => (
        <div key={`col-${c}`} className="label">
          {c}
        </div>
      ))}
    </div>
  );
};

export { Board };
