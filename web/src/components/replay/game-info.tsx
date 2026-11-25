import { NodeBadge, RemoteBadge } from '@/components/badge/badge';
import type { TColor, TGame } from '@/types/games.types';
import type { FC } from 'react';
import { ENDINGS } from './replay.data';

const GameInfo: FC<{ game: TGame; versionOf: (team: string, build?: string) => string }> = ({
  game,
  versionOf,
}) => {
  const loser = game.winner === 'red' ? 'black' : 'red';
  const ending = ENDINGS[game.reason]?.(game[loser].team) ?? game.reason;
  const player = (color: TColor): React.ReactNode => (
    <>
      <strong>
        {[game[color].team, versionOf(game[color].team, game[color].build)]
          .filter(Boolean)
          .join(' ')}
      </strong>
      {game[color].remote && <RemoteBadge />}
    </>
  );
  return (
    <>
      {player('red')}
      {' (rouge) contre '}
      {player('black')}
      {` (noir) : ${game[game.winner].team} gagne, ${ending}.`}
      {game.node !== undefined && game.node !== '' && <NodeBadge />}
    </>
  );
};

export { GameInfo };
