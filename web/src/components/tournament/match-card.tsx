import { LiveLink } from '@/components/live/live-link';
import { percent, winProbability } from '@/lib/predict';
import { pageUrl } from '@/lib/url';
import { cn } from '@/lib/utils';
import type { TRunningGame } from '@/types/games.types';
import type { TModel } from '@/types/summary.types';
import type { TMatch, TMatchSource, TTournament } from '@/types/tournament.types';
import type { FC, ReactNode } from 'react';
import { MATCH_STATUS } from './tournament.utils';

const FINAL_BEST_OF = 5;
const hasText = (value: string | null | undefined): value is string =>
  value !== null && value !== undefined && value !== '';
type Props = {
  match: TMatch;
  view: TTournament;
  seeds: Map<string, number>;
  byId: Map<string, TMatch>;
  model: TModel | null | undefined;
  running: TRunningGame[];
  // "équipe v2 · 1534 Elo" pour l'infobulle.
  describe: (team: string) => string;
};

function awaiting(
  source: TMatchSource | undefined,
  bye: boolean,
  byId: Map<string, TMatch>,
): string {
  if (source === undefined) {
    return bye ? 'exempt' : 'à venir';
  }
  const reference = source.winner ?? source.loser;
  const previous = reference === undefined ? undefined : byId.get(reference);
  if (
    bye ||
    (previous?.status === 'done' &&
      previous.games.length === 0 &&
      (hasText(source.loser) || !hasText(previous.winner)))
  ) {
    return 'exempt';
  }
  if (hasText(source.winner)) {
    return `vainqueur ${source.winner}`;
  }
  if (hasText(source.loser)) {
    return `perdant ${source.loser}`;
  }
  return 'à venir';
}

function slot(
  index: 0 | 1,
  { match, seeds, byId, model, describe }: Props,
  { bye, scored, open }: { bye: boolean; scored: boolean; open: boolean },
): ReactNode {
  const team = match.teams[index];
  let state = '';
  if (!hasText(team)) {
    state = 'tbd';
  } else if (!bye && match.winner === team) {
    state = 'win';
  } else if (!bye && hasText(match.winner)) {
    state = 'lose';
  }
  const chance = open ? winProbability(model, team, match.teams[index === 0 ? 1 : 0]) : null;
  return (
    <div className={cn('slot', state)}>
      <span className="seed">{hasText(team) ? String(seeds.get(team)) : ''}</span>
      <span className="name" title={hasText(team) ? describe(team) : ''}>
        {team ?? awaiting(match.from[index], bye, byId)}
      </span>
      {chance === null ? (
        <span className="score">{hasText(team) && scored ? String(match.score[index]) : ''}</span>
      ) : (
        <span
          className="score prob"
          title={`${percent(chance)} % de chances de gagner une manche, selon le modèle (couleurs moyennées)`}
        >
          {percent(chance)}%
        </span>
      )}
    </div>
  );
}

// Independent status, live-link, final-note and game-list display conditions add to this metric.
// oxlint-disable-next-line react-doctor/no-high-complexity-react-function
const MatchCard: FC<Props> = (props) => {
  const { match, view, running } = props;
  const bye = match.status === 'done' && match.games.length === 0;
  const from = match.from
    .flatMap((source) =>
      source.winner !== undefined && source.winner !== '' ? [source.winner] : [],
    )
    .join(',');

  if (bye && !hasText(match.teams[0]) && !hasText(match.teams[1])) {
    return <div className="match-wrap" data-id={match.id} data-from={from} />;
  }
  const scored = match.games.length > 0 || match.head_start.some(Boolean);
  const open =
    hasText(match.teams[0]) && hasText(match.teams[1]) && match.status !== 'done' && !scored;

  const label = [
    match.id,
    match.best_of === FINAL_BEST_OF
      ? 'meilleur de 5, le vainqueur du principal part à 1–0'
      : `meilleur de ${match.best_of}`,
  ];
  const live =
    match.status === 'running' &&
    running.find(
      (game) =>
        game.id !== undefined &&
        game.id !== '' &&
        game.tournament?.id === view.id &&
        game.tournament.match === match.id,
    );

  return (
    <div className="match-wrap" data-id={match.id} data-from={from}>
      <p className="match-id">
        {match.id}
        {hasText(MATCH_STATUS[match.status]) && (
          <span className={cn('match-state', match.status)}>{MATCH_STATUS[match.status]}</span>
        )}
        {live !== false && live !== undefined && (
          <LiveLink id={live.id ?? ''} title="Suivre la manche en direct" />
        )}
        {match.side === 'final' && <span className="match-note">meilleur de 5, avance de 1–0</span>}
      </p>
      <div
        className={cn('match', match.status, bye && 'bye')}
        aria-label={`${label.join(', ')} : ${match.teams.map((team) => team ?? 'à venir').join(' contre ')}`}
      >
        {slot(0, props, { bye, scored, open })}
        {slot(1, props, { bye, scored, open })}
      </div>
      {match.games.length > 0 && (
        <p className="match-games">
          Manches{' '}
          {match.games.map((id, index) => (
            <a key={id} href={pageUrl('./', { game: id })} title="Rejouer cette manche">
              {index + 1}
            </a>
          ))}
        </p>
      )}
    </div>
  );
};

export { MatchCard };
