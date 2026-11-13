import { Badge } from '@/components/badge/badge';
import { useScrollEdges } from '@/hooks/use-scroll-edges';
import { winProbability } from '@/lib/predict';
import { cn } from '@/lib/utils';
import type { TRunningGame } from '@/types/games.types';
import type { TRankingRow } from '@/lib/archives';
import type { TModel, TPaceStat } from '@/types/summary.types';
import { useRef, useState, type FC, type ReactNode, type SyntheticEvent } from 'react';
import { ELO_STATS, PACE_STATS, RECORD_STATS, type TEloStat, type TRecordStat } from './ranking.data';
import { RankingRow, type TDuel } from './ranking-row';

type Props = {
  // Lignes actives, et versions archivées choisies sous leur équipe.
  ratings: TRankingRow[];
  // Filtre des archives, sous le titre.
  children?: ReactNode;
  loaded: boolean;
  running: TRunningGame[];
  model: TModel | null | undefined;
  versionOf: (team: string, build?: string) => string;
  timeMs?: number;
  eloStat: TEloStat;
  recordStat: TRecordStat;
  paceStat: TPaceStat;
  onNextElo: () => void;
  onNextRecord: () => void;
  onNextPace: () => void;
  onOpenQueue: () => void;
};

const PING_MS = 1200;

const opponentOf = (e: SyntheticEvent) =>
  (e.target as Element).closest?.<HTMLElement>('[data-opponent]')?.dataset.opponent ?? null;

const RankingPanel: FC<Props> = ({
  ratings,
  loaded,
  running,
  model,
  versionOf,
  timeMs,
  eloStat,
  recordStat,
  paceStat,
  onNextElo,
  onNextRecord,
  onNextPace,
  onOpenQueue,
  children,
}) => {
  const scrollRef = useRef<HTMLDivElement>(null);
  const scroll = useScrollEdges(scrollRef);
  const rows = useRef(new Map<string, HTMLTableRowElement>());
  const [opponent, setOpponent] = useState<string | null>(null);
  const [ping, setPing] = useState<string | null>(null);

  const duels = new Map<string, TDuel>();
  for (const g of running) {
    if (!duels.has(g.red))
      duels.set(g.red, { id: g.id, opponent: g.black, color: 'red', chance: winProbability(model, g.red, g.black, 'red') });
    if (!duels.has(g.black))
      duels.set(g.black, { id: g.id, opponent: g.red, color: 'black', chance: winProbability(model, g.black, g.red, 'black') });
  }
  const active = ratings.filter((r) => !r.archived);
  const listed = new Set(active.map((r) => r.team));
  const ranks = new Map(active.map((r, i) => [r.team, i + 1]));

  const showOpponent = (e: SyntheticEvent) => setOpponent(opponentOf(e));
  const clearOpponent = () => setOpponent(null);
  const goToOpponent = (e: SyntheticEvent) => {
    const team = opponentOf(e);
    const row = team && rows.current.get(team);
    if (!row) return;
    row.scrollIntoView({ block: 'nearest' });
    setPing(team);
    setTimeout(() => setPing((current) => (current === team ? null : current)), PING_MS);
  };

  const [eloLabel, eloTitle] = ELO_STATS[eloStat];
  const [recordLabel, recordTitle] = RECORD_STATS[recordStat];
  const [paceLabel, paceTitle] = PACE_STATS[paceStat];

  return (
    <section className="panel ranking" aria-labelledby="ranking-title">
      <h2 id="ranking-title">Classement</h2>
      {children}
      <div
        ref={scrollRef}
        className={cn('scroll ranking-scroll', scroll.edges.less && 'less', scroll.edges.more && 'more')}
        tabIndex={0}
        aria-labelledby="ranking-title"
        onScroll={scroll.onScroll}
      >
        <table id="ranking">
          <thead>
            <tr>
              <th scope="col" className="num">
                #
              </th>
              <th scope="col">Équipe</th>
              <th scope="col" className="num">
                <button id="elo-stat" className="th-toggle" type="button" title={`${eloTitle}. Cliquer pour changer.`} onClick={onNextElo}>
                  {eloLabel}
                </button>
              </th>
              <th scope="col" className="num record">
                <button
                  id="record-stat"
                  className="th-toggle"
                  type="button"
                  title={`${recordTitle}. Cliquer pour changer.`}
                  onClick={onNextRecord}
                >
                  {recordLabel}
                </button>
              </th>
              <th scope="col" className="num pct">
                <abbr title="Pourcentage de victoires, version actuelle">% V</abbr>
              </th>
              <th scope="col" className="num pace">
                <button
                  id="pace-stat"
                  className="th-toggle"
                  type="button"
                  title={`${paceTitle}, version actuelle. Cliquer pour changer de mesure.`}
                  onClick={onNextPace}
                >
                  {paceLabel}
                </button>
              </th>
              <th scope="col" className="num last-col">
                <button
                  id="queue-open"
                  className="th-toggle"
                  type="button"
                  aria-haspopup="dialog"
                  title="Voir la file d'attente"
                  onClick={onOpenQueue}
                >
                  Dernière
                </button>
              </th>
            </tr>
          </thead>
          <tbody
            onPointerOver={showOpponent}
            onFocus={showOpponent}
            onPointerOut={clearOpponent}
            onBlur={clearOpponent}
            onClick={goToOpponent}
          >
            {ratings.map((r) => {
              if (r.archived)
                return (
                  <RankingRow
                    key={`${r.team}@${r.build}`}
                    rating={r}
                    version={versionOf(r.team, r.build)}
                    opponentListed={false}
                    eloStat={eloStat}
                    recordStat={recordStat}
                    paceStat={paceStat}
                  />
                );
              const duel = duels.get(r.team);
              return (
                <RankingRow
                  key={r.team}
                  ref={(node) => {
                    if (node) rows.current.set(r.team, node);
                    else rows.current.delete(r.team);
                  }}
                  rank={ranks.get(r.team)}
                  rating={r}
                  version={versionOf(r.team, r.build)}
                  duel={duel}
                  opponentListed={Boolean(duel && listed.has(duel.opponent))}
                  eloStat={eloStat}
                  recordStat={recordStat}
                  paceStat={paceStat}
                  highlight={ping === r.team ? 'ping' : opponent === r.team ? 'opponent' : undefined}
                />
              );
            })}
          </tbody>
        </table>
        <p id="ranking-empty" className="empty" hidden={!loaded || ratings.length > 0}>
          Aucune partie pour l'instant : le classement se remplit dès que deux équipes ont un jar accepté.
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
            Une ligne grisée marquée <Badge>bot</Badge> est un adversaire de référence, pas une équipe.
          </span>{' '}
          <span id="remote-note" hidden={!ratings.some((r) => r.remote)}>
            Une équipe marquée <Badge variant="remote">distant</Badge> joue depuis sa propre machine : le serveur ne
            voit pas son code, ne limite ni ses coeurs ni sa mémoire, et compte la latence réseau dans son temps.
          </span>{' '}
          Temps par coup du ladder : <span id="time-ms">{timeMs ?? '?'}</span> ms, contre 5000 ms au tournoi.
        </p>
      </details>
    </section>
  );
};

export { RankingPanel };
