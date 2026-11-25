import { Badge } from '@/components/badge/badge';
import { SectionToggle } from '@/components/controls/section-toggle';
import { useScrollEdges } from '@/hooks/use-scroll-edges';
import { winProbability } from '@/lib/predict';
import { cn } from '@/lib/utils';
import type { TRunningGame } from '@/types/games.types';
import type { TRankingRow } from '@/lib/archives';
import type { TModel, TPaceStat } from '@/types/summary.types';
import {
  useRef,
  useState,
  type FC,
  type PointerEvent,
  type ReactNode,
  type SyntheticEvent,
} from 'react';
import {
  ELO_STATS,
  PACE_STATS,
  RECORD_STATS,
  SORT_COLUMNS,
  rankingView,
  statOf,
  type TEloStat,
  type TRankingSort,
  type TRecordStat,
  type TSortColumn,
} from './ranking.data';
import { RankingRow, type TDuel } from './ranking-row';

type Props = {
  // Lignes actives, et versions archivées choisies placées à leur Elo.
  ratings: TRankingRow[];
  // Filtre des archives, sous le titre.
  children?: ReactNode;
  collapsed: boolean;
  onToggle: () => void;
  loaded: boolean;
  running: TRunningGame[];
  model: TModel | null | undefined;
  versionOf: (team: string, build?: string) => string;
  timeMs?: number;
  eloStat: TEloStat;
  recordStat: TRecordStat;
  paceStat: TPaceStat;
  sort: TRankingSort;
  onEloChange: (stat: TEloStat) => void;
  onRecordChange: (stat: TRecordStat) => void;
  onPaceChange: (stat: TPaceStat) => void;
  onSortChange: (sort: TRankingSort) => void;
  onOpenQueue: () => void;
};

type SortHeaderProps = {
  column: TSortColumn;
  label: string;
  title?: string;
  sort: TRankingSort;
  onSort: (column: TSortColumn) => void;
  className?: string;
};

const SortHeader: FC<SortHeaderProps> = ({ column, label, title, sort, onSort, className }) => (
  <th
    scope="col"
    className={className}
    aria-sort={sort.column === column ? sort.direction : undefined}
  >
    <button className="th-sort" type="button" title={title} onClick={() => onSort(column)}>
      {column === 'rank' && <span className="sr-only">Rang officiel </span>}
      {label}
      <span className="sort-direction" aria-hidden="true" />
    </button>
  </th>
);

const opponentOf = (e: SyntheticEvent) =>
  (e.target as Element).closest?.<HTMLElement>('[data-opponent]')?.dataset.opponent ?? null;

const RankingSection: FC<Props> = ({
  ratings,
  loaded,
  running,
  model,
  versionOf,
  timeMs,
  eloStat,
  recordStat,
  paceStat,
  sort,
  onEloChange,
  onRecordChange,
  onPaceChange,
  onSortChange,
  onOpenQueue,
  children,
  collapsed,
  onToggle,
}) => {
  const scrollRef = useRef<HTMLDivElement>(null);
  const scroll = useScrollEdges(scrollRef);
  const [opponent, setOpponent] = useState<string | null>(null);

  const duels = new Map<string, TDuel>();
  for (const g of running) {
    if (!duels.has(g.red))
      duels.set(g.red, {
        id: g.id,
        opponent: g.black,
        color: 'red',
        chance: winProbability(model, g.red, g.black, 'red'),
        tournament: g.tournament,
      });
    if (!duels.has(g.black))
      duels.set(g.black, {
        id: g.id,
        opponent: g.red,
        color: 'black',
        chance: winProbability(model, g.black, g.red, 'black'),
        tournament: g.tournament,
      });
  }
  const { rows: sorted, ranks } = rankingView(ratings, sort, {
    elo: eloStat,
    record: recordStat,
    pace: paceStat,
  });

  const showOpponent = (e: SyntheticEvent) => setOpponent(opponentOf(e));
  const clearOpponent = () => setOpponent(null);
  // Au tactile, c'est le focus de la pastille qui surligne.
  const mouseOnly = (handler: (e: PointerEvent) => void) => (e: PointerEvent) =>
    e.pointerType === 'mouse' && handler(e);

  const [eloLabel, eloTitle] = ELO_STATS[eloStat];
  const [recordLabel, recordTitle] = RECORD_STATS[recordStat];
  const [paceLabel, paceTitle] = PACE_STATS[paceStat];
  const labels = { ...SORT_COLUMNS, elo: eloLabel, record: recordLabel, pace: paceLabel };
  const onSort = (column: TSortColumn) =>
    onSortChange({
      column,
      direction:
        sort.column === column && sort.direction === 'ascending' ? 'descending' : 'ascending',
    });
  const sortProps = { sort, onSort };

  return (
    <section id="ranking-section" className="section ranking" aria-labelledby="ranking-title">
      <div className="ranking-heading">
        <SectionToggle
          id="ranking-title"
          collapsed={collapsed}
          controls="ranking-section"
          onToggle={onToggle}
        >
          Classement
        </SectionToggle>
        <button
          id="queue-open"
          className="queue-open"
          type="button"
          aria-haspopup="dialog"
          onClick={onOpenQueue}
        >
          File d’attente
        </button>
      </div>
      {children}
      <div className="ranking-tools" role="group" aria-label="Mesures affichées">
        <label htmlFor="elo-stat">
          <span>Score</span>
          <select
            id="elo-stat"
            value={eloStat}
            title={eloTitle}
            onChange={(e) => onEloChange(statOf(ELO_STATS, e.target.value, eloStat))}
          >
            {Object.entries(ELO_STATS).map(([value, [label, title]]) => (
              <option key={value} value={value} title={title}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <label htmlFor="record-stat">
          <span>Bilan</span>
          <select
            id="record-stat"
            value={recordStat}
            aria-label="Bilan et parties"
            title={recordTitle}
            onChange={(e) => onRecordChange(statOf(RECORD_STATS, e.target.value, recordStat))}
          >
            <option value="all" title={RECORD_STATS.all[1]}>
              Total
            </option>
            <option value="build" title={RECORD_STATS.build[1]}>
              Actuel
            </option>
          </select>
        </label>
        <label htmlFor="pace-stat">
          <span>Temps</span>
          <select
            id="pace-stat"
            value={paceStat}
            aria-label="Temps par coup"
            title={paceTitle}
            onChange={(e) => onPaceChange(statOf(PACE_STATS, e.target.value, paceStat))}
          >
            {Object.entries(PACE_STATS).map(([value, [label, title]]) => (
              <option key={value} value={value} title={title}>
                {label}
              </option>
            ))}
          </select>
        </label>
      </div>
      <p className="sr-only" role="status">
        {`Tri : ${labels[sort.column]}, ordre ${sort.direction === 'ascending' ? 'croissant' : 'décroissant'}.`}
      </p>
      <div
        ref={scrollRef}
        className={cn(
          'scroll ranking-scroll',
          scroll.edges.less && 'less',
          scroll.edges.more && 'more',
        )}
        tabIndex={0}
        role="region"
        aria-labelledby="ranking-title"
        onScroll={scroll.onScroll}
      >
        <table id="ranking">
          <caption className="sr-only">
            Classement et statistiques des équipes. Le numéro reste le rang officiel.
          </caption>
          <thead>
            <tr>
              <SortHeader
                {...sortProps}
                column="rank"
                label="#"
                title="Rang officiel selon l’Elo actuel"
                className="num"
              />
              <SortHeader {...sortProps} column="team" label="Équipe" className="team" />
              <SortHeader
                {...sortProps}
                column="elo"
                label={eloLabel}
                title={eloTitle}
                className="num"
              />
              <SortHeader
                {...sortProps}
                column="record"
                label={recordLabel}
                title={`${recordTitle}. Tri par victoires, puis moins de défaites.`}
                className="num record"
              />
              <SortHeader
                {...sortProps}
                column="games"
                label="Parties"
                title={
                  recordStat === 'build'
                    ? 'Nombre de parties, version actuelle'
                    : 'Nombre de parties, toutes versions'
                }
                className="num"
              />
              <SortHeader
                {...sortProps}
                column="pct"
                label="% V"
                title="Pourcentage de victoires, version actuelle"
                className="num pct"
              />
              <SortHeader
                {...sortProps}
                column="pace"
                label={paceLabel}
                title={`${paceTitle}, version actuelle`}
                className="num pace"
              />
              <SortHeader
                {...sortProps}
                column="last"
                label="Dernière"
                title="Date de la dernière partie"
                className="num last-col"
              />
            </tr>
          </thead>
          <tbody
            onPointerOver={mouseOnly(showOpponent)}
            onFocus={showOpponent}
            onPointerOut={mouseOnly(clearOpponent)}
            onBlur={clearOpponent}
          >
            {sorted.map((r) => {
              const active = !r.archived;
              const duel = active ? duels.get(r.team) : undefined;
              return (
                <RankingRow
                  key={active ? r.team : `${r.team}@${r.build}`}
                  rank={active ? ranks.get(r.team) : undefined}
                  rating={r}
                  version={versionOf(r.team, r.build)}
                  duel={duel}
                  eloStat={eloStat}
                  recordStat={recordStat}
                  paceStat={paceStat}
                  highlight={active && opponent === r.team}
                />
              );
            })}
          </tbody>
        </table>
        <p id="ranking-empty" className="empty" hidden={!loaded || ratings.length > 0}>
          Aucune partie pour l'instant : le classement se remplit dès que deux équipes ont un jar
          accepté.
        </p>
      </div>
      <details className="how">
        <summary>Comment l'Elo est calculé</summary>
        <p>
          Chaque équipe part à 1500. Un nouveau jar joue ses 20 premières parties à K = 40 (marque{' '}
          <Badge>provisoire</Badge>), puis à K = 20.{' '}
          <span id="anchor-note" hidden={!ratings.some((r) => r.anchor)}>
            Une <Badge variant="anchor">ancre</Badge> garde un rating fixe de 1500.
          </span>{' '}
          <span id="bot-note" hidden={!ratings.some((r) => !r.build)}>
            Une ligne grisée marquée <Badge>bot</Badge> est un adversaire de référence, pas une
            équipe.
          </span>{' '}
          <span id="remote-note" hidden={!ratings.some((r) => r.remote)}>
            Une équipe marquée <Badge variant="remote">distant</Badge> joue depuis sa propre machine
            : le serveur ne voit pas son code, ne limite ni ses coeurs ni sa mémoire, et compte la
            latence réseau dans son temps.
          </span>{' '}
          Temps par coup du ladder : <span id="time-ms">{timeMs ?? '?'}</span> ms, contre 5000 ms au
          tournoi.
        </p>
      </details>
    </section>
  );
};

export { RankingSection };
