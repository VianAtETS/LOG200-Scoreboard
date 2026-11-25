import { DuelIcon, LiveIcon } from '@/components/icons/icons';
import { Tooltip, useTooltip } from '@/components/tooltip/tooltip';
import { percent } from '@/lib/predict';
import { pageUrl } from '@/lib/url';
import { cn } from '@/lib/utils';
import type { TColor, TTournamentRef } from '@/types/games.types';
import { useRef } from 'react';
import type { FC, MouseEvent, PointerEvent } from 'react';

type Props = {
  team: string;
  opponent: string;
  color: TColor;
  chance: number | null;
  id?: string | undefined;
  tournament?: TTournamentRef | undefined;
};

// Au tactile, sans survol, le premier tap ouvre la bulle au lieu de suivre le lien.
const DuelBadge: FC<Props> = ({ team, opponent, color, chance, id, tournament }) => {
  const { anchorProps, tipProps, pin } = useTooltip<HTMLElement>();
  const pointer = useRef('mouse');
  const [red, black] = color === 'red' ? [team, opponent] : [opponent, team];
  const href = id !== undefined && id !== '' ? pageUrl('./', { game: id }) : undefined;
  const label = `Partie en cours contre ${opponent}${href === undefined ? '' : ', suivre en direct'}`;

  // detail 0 : Entrée au clavier, qui suit le lien.
  const handleClick = (event: MouseEvent<HTMLElement>): void => {
    if (event.detail === 0 || pointer.current === 'mouse') {
      return;
    }
    event.preventDefault();
    pin(event.currentTarget);
  };

  const props = {
    className: cn('duel-badge', color),
    'aria-label': label,
    'data-opponent': opponent,
    ...anchorProps,
    onPointerDown: (event: PointerEvent<HTMLElement>): void => {
      pointer.current = event.pointerType;
    },
    onClick: handleClick,
  };

  return (
    <>
      {href === undefined ? (
        // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role, jsx-a11y/no-noninteractive-tabindex, react/jsx-props-no-spreading -- A native image cannot receive keyboard focus; this label opens the tooltip on focus without inventing a link.
        <span role="img" tabIndex={0} {...props}>
          <DuelIcon />
        </span>
      ) : (
        // oxlint-disable-next-line react/jsx-props-no-spreading -- Link and non-link indicators share all tooltip interactions.
        <a href={href} {...props}>
          <DuelIcon />
          {/* oxlint-disable-next-line react/forbid-component-props -- The SVG class positions the live marker beside the duel icon. */}
          <LiveIcon className="duel-live" />
        </a>
      )}
      {/* oxlint-disable-next-line react/jsx-props-no-spreading -- The tooltip consumes the controller state as a unit. */}
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
        {href !== undefined &&
          (tipProps.pinned ? (
            <a className="primary tip-action" href={href}>
              {/* oxlint-disable-next-line react/forbid-component-props -- The SVG class marks the live action icon. */}
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
