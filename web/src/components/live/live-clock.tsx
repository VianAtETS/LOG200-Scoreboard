import { useInterval } from '@/hooks/use-interval';
import { mover } from '@/lib/live';
import type { TLiveGame } from '@/types/live.types';
import { useState } from 'react';
import type { FC } from 'react';

const TICK_MS = 250;

// Seule l'horloge bat au quart de seconde : le plateau ne se redessine qu'à chaque coup.
const LiveClock: FC<{ game: TLiveGame }> = ({ game }) => {
  const [now, setNow] = useState(() => performance.now());
  useInterval(() => setNow(performance.now()), game.ended ? null : TICK_MS);
  const count = game.moves.length;
  let text = `${count} coups`;
  if (!game.ended) {
    const left = Math.max(0, game.time_ms - (now - game.turnAt)) / 1000;
    const last = game.move_ms.at(-1);
    const thinking = `${game[mover(game)]} réfléchit : ${left.toFixed(1)} s`;
    text = [
      `Coup ${count + 1}`,
      thinking,
      ...(last === undefined ? [] : [`dernier en ${last} ms`]),
    ].join(' · ');
  }
  return (
    <p className="live-clock" data-id={game.id}>
      {text}
    </p>
  );
};

export { LiveClock };
