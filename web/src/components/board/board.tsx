import { useDescriptor } from '@/dao/hooks/use-descriptor';
import { renderers } from '@/games';
import { columnName, pieceOf, rowName } from '@/lib/game';
import type { TStep } from '@/lib/replay';
import { cn } from '@/lib/utils';
import type { TDescriptor, TRole } from '@/types/game.types';
import { Component, type CSSProperties, type FC, type ReactNode } from 'react';

export type BoardProps = {
  step: TStep;
  // Les rouges jouent les coups impairs : 1, 3, 5...
  index: number;
  label: string;
  id?: string;
  descriptor: TDescriptor;
};

// Classe du dernier coup marqué sur une case, selon le rôle de son effet.
const ROLE_CLASS: Record<TRole, string> = { from: 'from', to: 'to', effect: 'destroyed-now' };

// Plateau générique d'une position de positions() : la grille du descripteur, ses cases particulières,
// les pièces selon leur style, et le dernier coup marqué d'après ses effets. Aucune règle du jeu.
export const GridBoard: FC<BoardProps> = ({ step, index, label, id, descriptor: d }) => {
  const special = new Map((d.special ?? []).map((s) => [s.cell, s.style]));
  const roles = new Map<number, Set<TRole>>();
  for (const c of step.changes) roles.set(c.cell, (roles.get(c.cell) ?? new Set()).add(c.role));

  const cells = [];
  for (let y = 0; y < d.rows; y++) {
    cells.push(
      <div key={`row-${y}`} className="label">
        {rowName(d, y)}
      </div>
    );
    for (let x = 0; x < d.cols; x++) {
      const i = y * d.cols + x;
      const piece = pieceOf(d, step.cells[i]);
      const style = piece?.style ?? 'pawn';
      const marks = [...(roles.get(i) ?? [])].map((role) => ROLE_CLASS[role]);
      const drawn = style !== 'empty' && style !== 'hole';
      cells.push(
        <div
          key={i}
          className={cn('cell', style === 'hole' && 'hole', special.has(i) && `special special-${special.get(i)}`, marks)}
        >
          {drawn && (
            <div
              className={cn('pawn', piece?.side, style === 'king' && 'king', !piece?.side && 'neutral')}
              title={piece?.label}
            />
          )}
        </div>
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
      style={{ '--rows': d.rows, '--cols': d.cols } as CSSProperties}
    >
      {cells}
      <div />
      {Array.from({ length: d.cols }, (_, x) => (
        <div key={`col-${x}`} className="label">
          {columnName(x)}
        </div>
      ))}
    </div>
  );
};

// Un rendu propre au jeu qui plante retombe sur la grille : une partie reste toujours lisible.
class Fallback extends Component<{ fallback: ReactNode; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: unknown) {
    console.warn('rendu du plateau en échec, grille générique', error);
  }

  render() {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}

// Le plateau du jeu de l'instance : son rendu propre s'il en a un (web/src/games/), sinon la grille.
const Board: FC<Omit<BoardProps, 'descriptor'>> = (props) => {
  const descriptor = useDescriptor();
  const grid = <GridBoard {...props} descriptor={descriptor} />;
  const Own = renderers[descriptor.game];
  if (!Own) return grid;
  return (
    <Fallback key={descriptor.game} fallback={grid}>
      <Own {...props} descriptor={descriptor} />
    </Fallback>
  );
};

export { Board };
