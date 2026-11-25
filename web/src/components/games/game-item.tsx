import { NodeBadge, RemoteBadge } from '@/components/badge/badge';
import { REASONS } from '@/components/replay/replay.data';
import { fullFormat, roundDelta, shortWhen, signed } from '@/lib/format';
import { cn } from '@/lib/utils';
import type { TColor, TGameRow, TRatingChange, TSide } from '@/types/games.types';
import { useCallback } from 'react';
import type { FC, ReactNode } from 'react';

export type TSideOrder = 'color' | 'left' | 'right';

type Props = {
  game: TGameRow;
  rating: (side: TSide) => TRatingChange;
  current: boolean;
  selectedTeam?: string | undefined;
  sideOrder?: TSideOrder | undefined;
  onSelect: (game: TGameRow) => void;
};

const GameItem: FC<Props> = ({
  game,
  rating,
  current,
  selectedTeam,
  sideOrder = 'color',
  onSelect,
}) => {
  const handleSelectGame = useCallback((): void => onSelect(game), [game, onSelect]);
  let selectedColor: TColor | undefined;
  if (game.red.team === selectedTeam) {
    selectedColor = 'red';
  } else if (game.black.team === selectedTeam) {
    selectedColor = 'black';
  }
  let left: TColor = 'red';
  if (selectedColor && sideOrder !== 'color') {
    const opponent = selectedColor === 'red' ? 'black' : 'red';
    left = sideOrder === 'left' ? selectedColor : opponent;
  }
  const right: TColor = left === 'red' ? 'black' : 'red';
  const change = (color: TColor): number => roundDelta(rating(game[color])[1]);
  const who = (color: TColor): string =>
    `${game[color].team} (${color === 'red' ? 'rouge' : 'noir'}${game[color].remote ? ', distant' : ''}${change(color) ? `, ${signed(change(color))}` : ''})`;

  const side = (color: TColor): ReactNode => {
    const d = change(color);
    return (
      <span className={cn('side', color, game.winner === color && 'winner')}>
        <span>{game[color].team}</span>
        {game[color].remote && <RemoteBadge />}
        {d ? (
          <span
            className={cn('delta', d > 0 ? 'up' : 'down')}
            title={`Elo après la partie : ${Math.round(rating(game[color])[0])}`}
          >
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
        data-id={game.id}
        aria-label={`${who(left)} contre ${who(right)}, ${game[game.winner].team} gagne${game.node ? ', sur un nœud' : ''}`}
        aria-current={current ? 'true' : undefined}
        onClick={handleSelectGame}
      >
        <time dateTime={game.ts} title={fullFormat.format(new Date(game.ts))}>
          {shortWhen(game.ts)}
        </time>
        {side(left)}
        {side(right)}
        <span className="meta">
          {game.node && <NodeBadge />}
          {`${REASONS[game.reason] ?? game.reason}, ${game.moves} coups, ${Math.round(game.duration_ms / 1000)} s`}
        </span>
      </button>
    </li>
  );
};

export { GameItem };
