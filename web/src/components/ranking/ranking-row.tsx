import { Badge, RemoteBadge } from '@/components/badge/badge';
import type { TRankingRow } from '@/lib/archives';
import { ago, fullFormat } from '@/lib/format';
import { cn } from '@/lib/utils';
import type { TColor, TTournamentRef } from '@/types/games.types';
import type { TPaceStat, TTeamRating } from '@/types/summary.types';
import type { FC } from 'react';
import { DuelBadge } from './duel-badge';
import { PACE_WARN, recordOf, winRate } from './ranking.data';
import type { TEloStat, TRecordStat } from './ranking.data';

export type TDuel = {
  id?: string;
  opponent: string;
  color: TColor;
  chance: number | null;
  tournament?: TTournamentRef;
};

type Props = {
  // Absent pour une version archivée : hors classement actuel.
  rank?: number;
  rating: TRankingRow;
  // "v3", vide sans build.
  version: string;
  duel?: TDuel;
  eloStat: TEloStat;
  recordStat: TRecordStat;
  paceStat: TPaceStat;
  highlight?: boolean;
};

const RankingRow: FC<Props> = ({
  rank,
  rating: r,
  version: v,
  duel,
  eloStat,
  recordStat,
  paceStat,
  highlight,
}) => {
  const bot = !r.build;
  const all = recordOf(r, 'all');
  const build = recordOf(r, 'build');
  const shown = recordStat === 'build' ? build : all;
  const allText = `${all.wins}–${all.losses}`;
  const buildText = `${build.wins}–${build.losses}`;
  const pct = winRate(r);
  const elo = r[eloStat];

  return (
    <tr
      className={cn(
        bot && 'bot',
        rank === 1 && 'leader',
        r.archived && 'archived-row',
        highlight && 'opponent',
      )}
      data-team={r.team}
      title={r.archived ? 'Dernier Elo de cette version, hors classement actuel' : undefined}
    >
      <td
        className="num rank"
        title={rank ? 'Rang officiel selon l’Elo actuel' : 'Version archivée, hors classement'}
      >
        {rank ?? '–'}
      </td>
      <th scope="row" className="team" title={r.team}>
        <div className="team-line">
          <span className="team-name">{r.team}</span>
          {r.archived && (
            <Badge variant="archived" title="Version archivée : ne joue plus de nouveaux matchs">
              Archivé
            </Badge>
          )}
          {v && <Badge title={`Build ${r.build}`}>{v}</Badge>}
          {r.remote && <RemoteBadge />}
          {bot && <Badge title="Adversaire de référence">bot</Badge>}
          {r.anchor && <Badge variant="anchor">ancre</Badge>}
          {r.provisional && <Badge>provisoire</Badge>}
          {duel && (
            <DuelBadge
              team={r.team}
              opponent={duel.opponent}
              color={duel.color}
              chance={duel.chance}
              id={duel.id}
              tournament={duel.tournament}
            />
          )}
        </div>
      </th>
      <td className="num elo">{elo === undefined ? '' : Math.round(elo)}</td>
      <td
        className="num record"
        title={
          r.archived
            ? `${v}, version archivée uniquement : ${buildText}`
            : `Toutes versions ${allText}, ${v || 'version actuelle'} ${buildText}`
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
      <td
        className="num pct"
        title={`${r.build_wins} victoires sur ${r.build_games} parties${v ? ` de ${v}` : ''}`}
      >
        {pct === undefined ? '' : `${Math.round(100 * pct)} %`}
      </td>
      <PaceCell rating={r} stat={paceStat} />
      <td className="num last-col" title={r.last ? fullFormat.format(new Date(r.last)) : ''}>
        {r.last ? ago(r.last) : ''}
      </td>
    </tr>
  );
};

const PaceCell = ({ rating: { pace }, stat }: { rating: TTeamRating; stat: TPaceStat }) => {
  // Sans `mean` : summary.json d'un ladder plus ancien.
  if (pace?.[stat] === undefined) {
    return <td className="num pace" />;
  }
  const mean = pace.mean === undefined ? '' : `Moyenne ${pace.mean} ms, `;
  const title = `${mean}médiane ${pace.p50} ms, p95 ${pace.p95} ms, p99 ${pace.p99} ms, max ${pace.max} ms, ${pace.count} coups`;
  return (
    <td className={cn('num pace', pace.p99 >= PACE_WARN * pace.limit && 'warn')} title={title}>
      {pace[stat]} ms
    </td>
  );
};

export { RankingRow };
