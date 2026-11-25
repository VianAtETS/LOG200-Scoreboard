import type { TLiveGame } from '@/types/live.types';
import type { FC } from 'react';
import { ENDINGS } from './replay.data';

const LiveInfo: FC<{ game: TLiveGame }> = ({ game }) => {
  const { winner, reason, cancelled, interrupted } = game.ended ?? {};
  const loser = winner === 'red' ? game.black : game.red;
  let state = 'en direct';
  if (game.ended) {
    if (cancelled) {
      state = 'partie annulée';
    } else if (interrupted) {
      state = 'direct interrompu';
    } else {
      const winnerName = winner === undefined ? undefined : game[winner];
      const ending = reason === undefined ? reason : (ENDINGS[reason]?.(loser) ?? reason);
      state = `${winnerName} gagne, ${ending}`;
    }
  }
  return (
    <>
      <strong>{game.red}</strong>
      {' (rouge) contre '}
      <strong>{game.black}</strong>
      {` (noir) : ${state}.`}
    </>
  );
};

export { LiveInfo };
