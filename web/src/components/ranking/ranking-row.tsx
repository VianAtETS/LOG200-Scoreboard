import { Badge, RemoteBadge } from '@/components/badge/badge';
import { LiveLink } from '@/components/live/live-link';
import type { TRankingRow } from '@/lib/archives';
import { ago, fullFormat } from '@/lib/format';
import { cn } from '@/lib/utils';
import type { TColor } from '@/types/games.types';
import type { TPaceStat, TTeamRating } from '@/types/summary.types';
import { forwardRef } from 'react';
import { DuelBadge } from './duel-badge';
import { PACE_WARN, type TEloStat, type TRecordStat } from './ranking.data';

export type TDuel = { id?: string; opponent: string; color: TColor; chance: number | null };

type Props = {
  // Absent pour une version archivée : hors classement actuel.
  rank?: number;
  rating: TRankingRow;
  // "v3", vide sans build.
  version: string;
  duel?: TDuel;
  opponentListed: boolean;
  eloStat: TEloStat;
  recordStat: TRecordStat;
  paceStat: TPaceStat;
  highlight?: 'opponent' | 'ping';
};

const RankingRow = forwardRef<HTMLTableRowElement, Props>(
  ({ rank, rating: r, version: v, duel, opponentListed, eloStat, recordStat, paceStat, highlight }, ref) => {
    const bot = !r.build;
    const all = `${r.wins}–${r.losses}`;
    const build = `${r.build_wins}–${r.build_games - r.build_wins}`;

    return (
      <tr
        ref={ref}
        className={cn(bot && 'bot', r.archived && 'archived-row', highlight)}
        data-team={r.team}
        title={r.archived ? 'Dernier Elo de cette version, hors classement actuel' : undefined}
      >
        <td className="num rank">{rank ?? '—'}</td>
        <td className="team" title={r.team}>
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
              <DuelBadge opponent={duel.opponent} color={duel.color} listed={opponentListed} chance={duel.chance} />
            )}
            {duel?.id && <LiveLink id={duel.id} title="Suivre la partie en direct" />}
          </div>
        </td>
        <td className="num elo">{Math.round(r[eloStat] ?? r.rating)}</td>
        <td
          className="num record"
          title={r.archived ? `${v}, version archivée uniquement : ${build}` : `Toutes versions ${all}, ${v || 'version actuelle'} ${build}`}
        >
          {recordStat === 'build' ? build : all}
        </td>
        <td className="num pct" title={`${r.build_wins} victoires sur ${r.build_games} parties${v ? ` de ${v}` : ''}`}>
          {Math.round((100 * r.build_wins) / r.build_games)} %
        </td>
        <PaceCell rating={r} stat={paceStat} />
        <td className="num last-col" title={r.last ? fullFormat.format(new Date(r.last)) : ''}>
          {r.last ? ago(r.last) : ''}
        </td>
      </tr>
    );
  }
);
RankingRow.displayName = 'RankingRow';

const PaceCell = ({ rating: { pace }, stat }: { rating: TTeamRating; stat: TPaceStat }) => {
  // Sans `mean` : summary.json d'un ladder plus ancien.
  if (pace?.[stat] === undefined) return <td className="num pace" />;
  const mean = pace.mean === undefined ? '' : `Moyenne ${pace.mean} ms, `;
  const title = `${mean}médiane ${pace.p50} ms, p95 ${pace.p95} ms, p99 ${pace.p99} ms, max ${pace.max} ms, ${pace.count} coups`;
  return (
    <td className={cn('num pace', pace.p99 >= PACE_WARN * pace.limit && 'warn')} title={title}>
      {pace[stat]} ms
    </td>
  );
};

export { RankingRow };
