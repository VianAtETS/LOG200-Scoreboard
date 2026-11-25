import { CloseIcon } from '@/components/icons/icons';
import { fullFormat, ordinal } from '@/lib/format';
import { cn } from '@/lib/utils';
import type { TRatingRow } from '@/types/games.types';
import { useLayoutEffect, useRef } from 'react';
import type { FC } from 'react';
import type { TRankPoint } from '@/lib/bump';
import type { TGeometry } from './bump.utils';

const TIP_OFFSET = 14;
const muted = 'var(--color-text-muted)';

type Props = {
  game: TRatingRow | null;
  col: number | null;
  count: number;
  rows: { team: string; p: TRankPoint; before: TRankPoint | undefined }[];
  team: string | null;
  palette: Map<string, string>;
  left: number;
  geom: TGeometry | null;
  winner: string | null;
  matchup: string;
  loser: string;
  versionOf: (team: string, build?: string) => string;
  hide: () => void;
};

function rankDifference(rank: number, previous: number | undefined): string {
  if (previous === undefined || previous === rank) {
    return '';
  }
  return rank < previous ? ' ▲' : ' ▼';
}

export const BumpTooltip: FC<Props> = ({
  game,
  col,
  count,
  rows,
  team,
  palette,
  left,
  geom,
  winner,
  matchup,
  loser,
  versionOf,
  hide,
}) => {
  const tip = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const node = tip.current;
    if (!node || !geom || col === null) {
      return;
    }
    const width = node.offsetWidth;
    node.style.left = `${left + TIP_OFFSET + width <= geom.width ? left + TIP_OFFSET : Math.max(0, left - TIP_OFFSET - width)}px`;
  });
  return (
    <div ref={tip} id="bump-tip" className="bump-tip" aria-hidden="true" hidden={!game}>
      {game && col !== null && (
        <>
          <p className="tip-when">
            {`${fullFormat.format(new Date(game.ts))} · partie ${col + 1} sur ${count}`}
          </p>
          <p className="tip-game">
            {winner !== null && <strong>{winner}</strong>}
            {winner === null ? matchup : ` bat ${loser}`}
          </p>
          <ol className="tip-rows">
            {rows.map(({ team: rowTeam, p: pt, before }) => {
              const move = rankDifference(pt.rank, before?.rank);
              const played = rowTeam === game.red.team || rowTeam === game.black.team;
              return (
                <li key={rowTeam} className={cn(rowTeam === team && 'on', played && 'played')}>
                  <span
                    className="chip-dot"
                    // oxlint-disable-next-line react-perf/jsx-no-new-object-as-prop -- The swatch is a native element and uses the current palette.
                    style={{ background: palette.get(rowTeam) ?? muted }}
                  />
                  <span className="tip-rank">{`${ordinal(pt.rank)}${move}`}</span>
                  <span className="tip-team">
                    {[rowTeam, pt.bot ? 'bot' : versionOf(rowTeam, pt.build)]
                      .filter(Boolean)
                      .join(' ')}
                  </span>
                  <span className="tip-elo">{Math.round(pt.rating)} Elo</span>
                </li>
              );
            })}
          </ol>
        </>
      )}
      <button
        type="button"
        className="tip-dismiss"
        tabIndex={-1}
        aria-label="Fermer"
        onClick={hide}
      >
        <CloseIcon />
      </button>
    </div>
  );
};
