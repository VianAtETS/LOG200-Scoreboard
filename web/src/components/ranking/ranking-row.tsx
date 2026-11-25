import { Badge, RemoteBadge } from '@/components/badge/badge';
import type { TRankingRow } from '@/lib/archives';
import { ago, fullFormat } from '@/lib/format';
import { cn } from '@/lib/utils';
import type { TColor, TTournamentRef } from '@/types/games.types';
import type { TPaceStat, TTeamRating } from '@/types/summary.types';
import type { FC, ReactNode } from 'react';
import { DuelBadge } from './duel-badge';
import { PACE_WARN, recordOf, winRate } from './ranking.data';
import type { TEloStat, TRecordStat } from './ranking.data';

export type TDuel = {
  id?: string | undefined;
  opponent: string;
  color: TColor;
  chance: number | null;
  tournament?: TTournamentRef | undefined;
};

type Props = {
  // Absent pour une version archivée : hors classement actuel.
  rank?: number | undefined;
  rating: TRankingRow;
  // "v3", vide sans build.
  version: string;
  duel?: TDuel | undefined;
  eloStat: TEloStat;
  recordStat: TRecordStat;
  paceStat: TPaceStat;
  highlight?: boolean | undefined;
};

const teamBadges = (
  rating: TRankingRow,
  version: string,
  duel: TDuel | undefined,
  bot: boolean,
): ReactNode => (
  <>
    {rating.archived && (
      <Badge variant="archived" title="Version archivée : ne joue plus de nouveaux matchs">
        Archivé
      </Badge>
    )}
    {version && <Badge title={`Build ${rating.build}`}>{version}</Badge>}
    {rating.remote && <RemoteBadge />}
    {bot && <Badge title="Adversaire de référence">bot</Badge>}
    {rating.anchor && <Badge variant="anchor">ancre</Badge>}
    {rating.provisional && <Badge>provisoire</Badge>}
    {duel && (
      <DuelBadge
        team={rating.team}
        opponent={duel.opponent}
        color={duel.color}
        chance={duel.chance}
        id={duel.id}
        tournament={duel.tournament}
      />
    )}
  </>
);

const PaceCell = ({
  rating: { pace },
  stat,
}: {
  rating: TTeamRating;
  stat: TPaceStat;
}): ReactNode => {
  // Sans `mean` : summary.json d'un ladder plus ancien.
  if (pace?.[stat] === undefined) {
    return <td className="num pace" aria-label="Temps indisponible" />;
  }
  const mean = pace.mean === undefined ? '' : `Moyenne ${pace.mean} ms, `;
  const title = `${mean}médiane ${pace.p50} ms, p95 ${pace.p95} ms, p99 ${pace.p99} ms, max ${pace.max} ms, ${pace.count} coups`;
  return (
    <td className={cn('num pace', pace.p99 >= PACE_WARN * pace.limit && 'warn')} title={title}>
      {pace[stat]} ms
    </td>
  );
};

const lastCell = (last: TTeamRating['last']): ReactNode => (
  <td
    className="num last-col"
    title={last !== undefined && last !== '' ? fullFormat.format(new Date(last)) : ''}
  >
    {last !== undefined && last !== '' ? ago(last) : ''}
  </td>
);

function recordCells(rating: TRankingRow, version: string, recordStat: TRecordStat): ReactNode {
  const all = recordOf(rating, 'all');
  const build = recordOf(rating, 'build');
  const shown = recordStat === 'build' ? build : all;
  const allText = `${all.wins}–${all.losses}`;
  const buildText = `${build.wins}–${build.losses}`;
  return (
    <>
      <td
        className="num record"
        title={
          rating.archived
            ? `${version}, version archivée uniquement : ${buildText}`
            : `Toutes versions ${allText}, ${version || 'version actuelle'} ${buildText}`
        }
      >
        {shown.wins}–{shown.losses}
      </td>
      <td
        className="num games-count"
        title={
          recordStat === 'build' ? 'Parties de la version actuelle' : 'Parties toutes versions'
        }
      >
        {shown.games}
      </td>
    </>
  );
}

// oxlint-disable-next-line react/no-multi-comp -- PaceCell is specific to this row.
const RankingRow: FC<Props> = ({
  rank,
  rating,
  version,
  duel,
  eloStat,
  recordStat,
  paceStat,
  highlight,
}) => {
  const bot = rating.build === undefined || rating.build === '';
  const pct = winRate(rating);
  const elo = rating[eloStat];
  const { last } = rating;

  return (
    <tr
      className={cn(
        bot && 'bot',
        rank === 1 && 'leader',
        rating.archived && 'archived-row',
        highlight && 'opponent',
      )}
      data-team={rating.team}
      title={rating.archived ? 'Dernier Elo de cette version, hors classement actuel' : undefined}
    >
      <td
        className="num rank"
        title={
          rank === undefined
            ? 'Version archivée, hors classement'
            : 'Rang officiel selon l’Elo actuel'
        }
      >
        {rank ?? '–'}
      </td>
      <th scope="row" className="team" title={rating.team}>
        <div className="team-line">
          <span className="team-name">{rating.team}</span>
          {teamBadges(rating, version, duel, bot)}
        </div>
      </th>
      <td className="num elo">{elo === undefined ? '' : Math.round(elo)}</td>
      {recordCells(rating, version, recordStat)}
      <td
        className="num pct"
        title={`${rating.build_wins} victoires sur ${rating.build_games} parties${version ? ` de ${version}` : ''}`}
      >
        {pct === undefined ? '' : `${Math.round(100 * pct)} %`}
      </td>
      <PaceCell rating={rating} stat={paceStat} />
      {lastCell(last)}
    </tr>
  );
};

export { RankingRow };
