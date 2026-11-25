import { DuelIcon, LiveIcon } from '@/components/icons/icons';
import { Tooltip, useTooltip } from '@/components/tooltip/tooltip';
import { percent } from '@/lib/predict';
import { pageUrl } from '@/lib/url';
import { cn } from '@/lib/utils';
import type { TColor, TTournamentRef } from '@/types/games.types';
import { useRef, type FC, type MouseEvent } from 'react';

type Props = {
  team: string;
  opponent: string;
  color: TColor;
  chance: number | null;
  id?: string;
  tournament?: TTournamentRef;
};

// Au tactile, sans survol, le premier tap ouvre la bulle au lieu de suivre le lien.
const DuelBadge: FC<Props> = ({ team, opponent, color, chance, id, tournament }) => {
  const { anchorProps, tipProps, pin } = useTooltip<HTMLElement>();
  const pointer = useRef('mouse');
  const [red, black] = color === 'red' ? [team, opponent] : [opponent, team];
  const href = id && pageUrl('./', { game: id });
  const label = `Partie en cours contre ${opponent}${id ? ', suivre en direct' : ''}`;

  // detail 0 : Entrée au clavier, qui suit le lien.
  const handleClick = (e: MouseEvent<HTMLElement>) => {
    if (e.detail === 0 || pointer.current === 'mouse') return;
    e.preventDefault();
    pin(e.currentTarget);
  };

  const props = {
    className: cn('duel-badge', color),
    'aria-label': label,
    'data-opponent': opponent,
    ...anchorProps,
    onPointerDown: (e: { pointerType: string }) => (pointer.current = e.pointerType),
    onClick: handleClick,
  };

  return (
    <>
      {href ? (
        <a href={href} {...props}>
          <DuelIcon />
          <LiveIcon className="duel-live" />
        </a>
      ) : (
        <span role="img" tabIndex={0} {...props}>
          <DuelIcon />
        </span>
      )}
      <Tooltip {...tipProps}>
        <p className="tip-head">
          {tournament
            ? `Tournoi, match ${tournament.match}, partie ${tournament.game}`
            : 'Partie en cours'}
        </p>
        <p className="tip-match">
          <span className="tip-side">
            <span className="tip-pawn red" aria-hidden="true" />
            {red}
          </span>
          <span className="tip-versus">contre</span>
          <span className="tip-side">
            <span className="tip-pawn black" aria-hidden="true" />
            {black}
          </span>
        </p>
        {chance !== null && (
          <p className="tip-odds">
            <strong>{percent(chance)} %</strong> de chances pour {team} selon le modèle
          </p>
        )}
        {href &&
          (tipProps.pinned ? (
            <a className="primary tip-action" href={href}>
              <LiveIcon className="duel-live" />
              Suivre en direct
            </a>
          ) : (
            <p className="tip-hint">Cliquer pour suivre la partie en direct</p>
          ))}
      </Tooltip>
    </>
  );
};

export { DuelBadge };
