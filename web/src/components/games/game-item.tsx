import { NodeBadge, RemoteBadge } from '@/components/badge/badge';
import { REASONS } from '@/components/replay/replay.data';
import { fullFormat, roundDelta, shortWhen, signed } from '@/lib/format';
import { cn } from '@/lib/utils';
import type { TColor, TGameRow, TRatingChange, TSide } from '@/types/games.types';
import type { FC } from 'react';

export type TSideOrder = 'color' | 'left' | 'right';

type Props = {
  game: TGameRow;
  rating: (side: TSide) => TRatingChange;
  current: boolean;
  selectedTeam?: string;
  sideOrder?: TSideOrder;
  onSelect: (game: TGameRow) => void;
};

const GameItem: FC<Props> = ({ game: g, rating, current, selectedTeam, sideOrder = 'color', onSelect }) => {
  const selectedColor = g.red.team === selectedTeam ? 'red' : g.black.team === selectedTeam ? 'black' : undefined;
  let left: TColor = 'red';
  if (selectedColor && sideOrder !== 'color') {
    const opponent = selectedColor === 'red' ? 'black' : 'red';
    left = sideOrder === 'left' ? selectedColor : opponent;
  }
  const right: TColor = left === 'red' ? 'black' : 'red';
  const change = (color: TColor) => roundDelta(rating(g[color])[1]);
  const who = (color: TColor) =>
    `${g[color].team} (${color === 'red' ? 'rouge' : 'noir'}${g[color].remote ? ', distant' : ''}${change(color) ? `, ${signed(change(color))}` : ''})`;

  const side = (color: TColor) => {
    const d = change(color);
    return (
      <span className={cn('side', color, g.winner === color && 'winner')}>
        <span>{g[color].team}</span>
        {g[color].remote && <RemoteBadge />}
        {d ? (
          <span className={cn('delta', d > 0 ? 'up' : 'down')} title={`Elo après la partie : ${Math.round(rating(g[color])[0])}`}>
            {signed(d)}
          </span>
        ) : null}
      </span>
    );
  };

  return (
    <li>
      <button
        type="button"
        className="game"
        data-id={g.id}
        aria-label={`${who(left)} contre ${who(right)}, ${g[g.winner].team} gagne${g.node ? ', sur un nœud' : ''}`}
        aria-current={current ? 'true' : undefined}
        onClick={() => onSelect(g)}
      >
        <time dateTime={g.ts} title={fullFormat.format(new Date(g.ts))}>
          {shortWhen(g.ts)}
        </time>
        {side(left)}
        {side(right)}
        <span className="meta">
          {g.node && <NodeBadge />}
          {`${REASONS[g.reason] ?? g.reason}, ${g.moves} coups, ${Math.round(g.duration_ms / 1000)} s`}
        </span>
      </button>
    </li>
  );
};

export { GameItem };
