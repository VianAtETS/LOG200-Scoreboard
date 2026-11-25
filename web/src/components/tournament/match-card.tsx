import { LiveLink } from '@/components/live/live-link';
import { percent, winProbability } from '@/lib/predict';
import { pageUrl } from '@/lib/url';
import { cn } from '@/lib/utils';
import type { TRunningGame } from '@/types/games.types';
import type { TModel } from '@/types/summary.types';
import type { TMatch, TMatchSource, TTournament } from '@/types/tournament.types';
import type { FC } from 'react';
import { MATCH_STATUS } from './tournament.utils';

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

const MatchCard: FC<Props> = ({ match: m, view, seeds, byId, model, running, describe }) => {
  const bye = m.status === 'done' && !m.games.length;
  const from = m.from.flatMap((f) => (f.winner ? [f.winner] : [])).join(',');

  if (bye && !m.teams[0] && !m.teams[1])
    return <div className="match-wrap" data-id={m.id} data-from={from} />;

  const awaiting = (source: TMatchSource) => {
    const previous = byId.get((source.winner ?? source.loser)!);
    if (
      bye ||
      (previous?.status === 'done' && !previous.games.length && (source.loser || !previous.winner))
    )
      return 'exempt';
    return source.winner
      ? `vainqueur ${source.winner}`
      : source.loser
        ? `perdant ${source.loser}`
        : 'à venir';
  };
  const scored = m.games.length > 0 || m.head_start.some(Boolean);
  const open = m.teams[0] && m.teams[1] && m.status !== 'done' && !scored;

  const slot = (i: 0 | 1) => {
    const team = m.teams[i];
    const state = !team ? 'tbd' : bye ? '' : m.winner === team ? 'win' : m.winner ? 'lose' : '';
    const chance = open ? winProbability(model, team, m.teams[1 - i]) : null;
    return (
      <div className={cn('slot', state)}>
        <span className="seed">{team ? String(seeds.get(team)) : ''}</span>
        <span className="name" title={team ? describe(team) : ''}>
          {team ?? awaiting(m.from[i])}
        </span>
        {chance !== null ? (
          <span
            className="score prob"
            title={`${percent(chance)} % de chances de gagner une manche, selon le modèle (couleurs moyennées)`}
          >
            {percent(chance)}%
          </span>
        ) : (
          <span className="score">{team && scored ? String(m.score[i]) : ''}</span>
        )}
      </div>
    );
  };

  const label = [
    m.id,
    m.best_of === 5
      ? 'meilleur de 5, le vainqueur du principal part à 1–0'
      : `meilleur de ${m.best_of}`,
  ];
  const live =
    m.status === 'running' &&
    running.find((g) => g.id && g.tournament?.id === view.id && g.tournament.match === m.id);

  return (
    <div className="match-wrap" data-id={m.id} data-from={from}>
      <p className="match-id">
        {m.id}
        {MATCH_STATUS[m.status] && (
          <span className={cn('match-state', m.status)}>{MATCH_STATUS[m.status]}</span>
        )}
        {live && <LiveLink id={live.id!} title="Suivre la manche en direct" />}
        {m.side === 'final' && <span className="match-note">meilleur de 5, avance de 1–0</span>}
      </p>
      <div
        className={cn('match', m.status, bye && 'bye')}
        aria-label={`${label.join(', ')} : ${m.teams.map((t) => t ?? 'à venir').join(' contre ')}`}
      >
        {slot(0)}
        {slot(1)}
      </div>
      {m.games.length > 0 && (
        <p className="match-games">
          Manches{' '}
          {m.games.map((id, k) => (
            <a key={id} href={pageUrl('./', { game: id })} title="Rejouer cette manche">
              {k + 1}
            </a>
          ))}
        </p>
      )}
    </div>
  );
};

export { MatchCard };
