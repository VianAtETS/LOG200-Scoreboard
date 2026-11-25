import { Board } from '@/components/board/board';
import { useInterval } from '@/hooks/use-interval';
import { mover } from '@/lib/live';
import { percent, winProbability } from '@/lib/predict';
import { positions } from '@/lib/replay';
import { pageUrl } from '@/lib/url';
import { cn } from '@/lib/utils';
import type { TLiveGame } from '@/types/live.types';
import type { TModel } from '@/types/summary.types';
import type { TTournament } from '@/types/tournament.types';
import { memo, useState } from 'react';
import type { FC } from 'react';

const TICK_MS = 250;

type Props = {
  game: TLiveGame;
  model: TModel | null | undefined;
  bracket?: TTournament;
};

function context(g: TLiveGame, bracket?: TTournament) {
  if (!g.tournament) {
    return 'Ladder';
  }
  const { match, game } = g.tournament;
  const m = bracket?.matches.find((x) => x.id === match);
  const score =
    m?.teams[0] && m.teams[1] ? ` · ${m.teams[0]} ${m.score[0]}–${m.score[1]} ${m.teams[1]}` : '';
  return `Tournoi · ${match} · manche ${game}${score}`;
}

function result({ ended, red, black }: TLiveGame) {
  if (ended!.cancelled) {
    return 'Partie annulée';
  }
  if (ended!.interrupted) {
    return 'Direct interrompu';
  }
  return `${ended!.winner === 'red' ? red : black} gagne`;
}

// Un clic ouvre la partie dans le replay de la page Classement, qui la suit en direct.
const LiveCard: FC<Props> = memo(({ game: g, model, bracket }) => {
  const steps = positions(g);
  const index = steps.length - 1;
  const chance = winProbability(model, g.red, g.black, 'red');

  return (
    <a className={cn('live-game', g.ended && 'ended')} href={pageUrl('./', { game: g.id })}>
      <p className="live-teams">
        <strong className="red">{g.red}</strong>
        {' contre '}
        <strong className="black">{g.black}</strong>
        {chance !== null && (
          <span className="duel-prob" title="Chances du rouge selon le modèle, avant la partie">
            {` ${percent(chance)} %`}
          </span>
        )}
      </p>
      <p className="live-meta">{context(g, bracket)}</p>
      <Board
        step={steps[index]}
        index={index}
        label={`${g.red} contre ${g.black}, coup ${index}`}
      />
      <LiveClock game={g} />
      {g.ended && <p className="live-result">{result(g)}</p>}
    </a>
  );
});
LiveCard.displayName = 'LiveCard';

// Seule l'horloge bat au quart de seconde : le plateau ne se redessine qu'à chaque coup.
const LiveClock: FC<{ game: TLiveGame }> = ({ game: g }) => {
  const [now, setNow] = useState(() => performance.now());
  useInterval(() => setNow(performance.now()), g.ended ? null : TICK_MS);

  const n = g.moves.length;
  let text = `${n} coups`;
  if (!g.ended) {
    const left = Math.max(0, g.time_ms - (now - g.turnAt)) / 1000;
    const last = g.move_ms.at(-1);
    const thinking = `${g[mover(g)]} réfléchit : ${left.toFixed(1)} s`;
    text = [
      `Coup ${n + 1}`,
      thinking,
      ...(last === undefined ? [] : [`dernier en ${last} ms`]),
    ].join(' · ');
  }

  return (
    <p className="live-clock" data-id={g.id}>
      {text}
    </p>
  );
};

export { LiveCard };
