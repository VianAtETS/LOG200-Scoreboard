import { Board } from '@/components/board/board';
import { percent, winProbability } from '@/lib/predict';
import { positions } from '@/lib/replay';
import { pageUrl } from '@/lib/url';
import { cn } from '@/lib/utils';
import type { TLiveEnded, TLiveGame } from '@/types/live.types';
import type { TModel } from '@/types/summary.types';
import type { TTournament } from '@/types/tournament.types';
import { memo } from 'react';
import type { FC } from 'react';

import { LiveClock } from './live-clock';

type Props = {
  game: TLiveGame;
  model: TModel | null | undefined;
  bracket?: TTournament;
};

function context(liveGame: TLiveGame, bracket?: TTournament): string {
  if (!liveGame.tournament) {
    return 'Ladder';
  }
  const { match, game } = liveGame.tournament;
  const matched = bracket?.matches.find((item) => item.id === match);
  const score =
    matched !== undefined && (matched.teams[0] ?? '') !== '' && (matched.teams[1] ?? '') !== ''
      ? ` · ${matched.teams[0]} ${matched.score[0]}–${matched.score[1]} ${matched.teams[1]}`
      : '';
  return `Tournoi · ${match} · manche ${game}${score}`;
}

function result(ended: TLiveEnded, red: string, black: string): string {
  if (ended.cancelled) {
    return 'Partie annulée';
  }
  if (ended.interrupted) {
    return 'Direct interrompu';
  }
  return `${ended.winner === 'red' ? red : black} gagne`;
}

// Un clic ouvre la partie dans le replay de la page Classement, qui la suit en direct.
const LiveCard: FC<Props> = memo(({ game, model, bracket }) => {
  const steps = positions(game);
  const index = steps.length - 1;
  const step = steps[index];
  if (step === undefined) {
    return null;
  }
  const chance = winProbability(model, game.red, game.black, 'red');

  return (
    <a className={cn('live-game', game.ended && 'ended')} href={pageUrl('./', { game: game.id })}>
      <p className="live-teams">
        <strong className="red">{game.red}</strong>
        {' contre '}
        <strong className="black">{game.black}</strong>
        {chance !== null && (
          <span className="duel-prob" title="Chances du rouge selon le modèle, avant la partie">
            {` ${percent(chance)} %`}
          </span>
        )}
      </p>
      <p className="live-meta">{context(game, bracket)}</p>
      <Board step={step} index={index} label={`${game.red} contre ${game.black}, coup ${index}`} />
      <LiveClock game={game} />
      {game.ended && <p className="live-result">{result(game.ended, game.red, game.black)}</p>}
    </a>
  );
});
LiveCard.displayName = 'LiveCard';

export { LiveCard };
