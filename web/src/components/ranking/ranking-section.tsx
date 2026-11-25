import { Badge } from '@/components/badge/badge';
import { SectionToggle } from '@/components/controls/section-toggle';
import { useScrollEdges } from '@/hooks/use-scroll-edges';
import { winProbability } from '@/lib/predict';
import { cn } from '@/lib/utils';
import type { TRunningGame } from '@/types/games.types';
import type { TRankingRow } from '@/lib/archives';
import type { TModel, TPaceStat } from '@/types/summary.types';
import { useCallback, useMemo, useRef, useState } from 'react';
import type { FC, PointerEvent, ReactNode, SyntheticEvent } from 'react';
import { ELO_STATS, PACE_STATS, RECORD_STATS, SORT_COLUMNS, rankingView } from './ranking.data';
import type { TEloStat, TRankingSort, TRecordStat } from './ranking.data';
// oxlint-disable-next-line import/max-dependencies -- The ranking section composes controls, rows and scroll tracking.
import { RankingHeaders, RankingTools } from './ranking-controls';
import { RankingRow } from './ranking-row';
import type { TDuel } from './ranking-row';

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
  timeMs?: number | undefined;
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

const opponentOf = (ev: SyntheticEvent): string | null =>
  ev.target instanceof Element
    ? (ev.target.closest<HTMLElement>('[data-opponent]')?.dataset['opponent'] ?? null)
    : null;
function duelsByTeam(running: TRunningGame[], model: Props['model']): Map<string, TDuel> {
  const duels = new Map<string, TDuel>();
  for (const game of running) {
    for (const color of ['red', 'black'] as const) {
      const team = game[color];
      if (duels.has(team)) {
        continue;
      }
      const peer = game[color === 'red' ? 'black' : 'red'];
      duels.set(team, {
        id: game.id,
        opponent: peer,
        color,
        chance: winProbability(model, team, peer, color),
        tournament: game.tournament,
      });
    }
  }
  return duels;
}

function rankingNotes(ratings: TRankingRow[], timeMs: number | undefined): ReactNode {
  return (
    <details className="how">
      <summary>Comment l&apos;Elo est calculé</summary>
      <p>
        Chaque équipe part à 1500. Un nouveau jar joue ses 20 premières parties à K = 40 (marque{' '}
        <Badge>provisoire</Badge>), puis à K = 20.{' '}
        <span id="anchor-note" hidden={!ratings.some((rating) => rating.anchor)}>
          Une <Badge variant="anchor">ancre</Badge> garde un rating fixe de 1500.
        </span>{' '}
        <span
          id="bot-note"
          hidden={!ratings.some((rating) => rating.build === undefined || rating.build === '')}
        >
          Une ligne grisée marquée <Badge>bot</Badge> est un adversaire de référence, pas une
          équipe.
        </span>{' '}
        <span id="remote-note" hidden={!ratings.some((rating) => rating.remote)}>
          Une équipe marquée <Badge variant="remote">distant</Badge> joue depuis sa propre machine :
          le serveur ne voit pas son code, ne limite ni ses coeurs ni sa mémoire, et compte la
          latence réseau dans son temps.
        </span>{' '}
        Temps par coup du ladder : <span id="time-ms">{timeMs ?? '?'}</span> ms, contre 5000 ms au
        tournoi.
      </p>
    </details>
  );
}

type TOpponentHighlight = {
  opponent: string | null;
  showOpponent: (event: SyntheticEvent) => void;
  clearOpponent: () => void;
  handlePointerOver: (event: PointerEvent) => void;
  handlePointerOut: (event: PointerEvent) => void;
};

function useOpponentHighlight(): TOpponentHighlight {
  const [opponent, setOpponent] = useState<string | null>(null);
  const showOpponent = useCallback(
    (event: SyntheticEvent): void => setOpponent(opponentOf(event)),
    [],
  );
  const clearOpponent = useCallback((): void => setOpponent(null), []);
  const handlePointerOver = useCallback(
    (event: PointerEvent): void => {
      if (event.pointerType === 'mouse') {
        showOpponent(event);
      }
    },
    [showOpponent],
  );
  const handlePointerOut = useCallback(
    (event: PointerEvent): void => {
      if (event.pointerType === 'mouse') {
        clearOpponent();
      }
    },
    [clearOpponent],
  );
  return { opponent, showOpponent, clearOpponent, handlePointerOver, handlePointerOut };
}

function sortLabel(
  { column }: TRankingSort,
  eloStat: TEloStat,
  recordStat: TRecordStat,
  paceStat: TPaceStat,
): string {
  const labels = {
    ...SORT_COLUMNS,
    elo: ELO_STATS[eloStat][0],
    record: RECORD_STATS[recordStat][0],
    pace: PACE_STATS[paceStat][0],
  };
  return labels[column];
}

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
  const scrollRef = useRef<HTMLElement>(null);
  const { edges, onScroll: handleScroll } = useScrollEdges(scrollRef);

  const { opponent, showOpponent, clearOpponent, handlePointerOver, handlePointerOut } =
    useOpponentHighlight();
  const duels = useMemo(() => duelsByTeam(running, model), [running, model]);
  const { rows: sorted, ranks } = rankingView(ratings, sort, {
    elo: eloStat,
    record: recordStat,
    pace: paceStat,
  });

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
      <RankingTools
        eloStat={eloStat}
        recordStat={recordStat}
        paceStat={paceStat}
        onEloChange={onEloChange}
        onRecordChange={onRecordChange}
        onPaceChange={onPaceChange}
      />
      <output className="sr-only">
        {`Tri : ${sortLabel(sort, eloStat, recordStat, paceStat)}, ordre ${sort.direction === 'ascending' ? 'croissant' : 'décroissant'}.`}
      </output>
      <section
        ref={scrollRef}
        className={cn('scroll ranking-scroll', edges.less && 'less', edges.more && 'more')}
        // oxlint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- The scroll region must be reachable by keyboard.
        tabIndex={0}
        aria-labelledby="ranking-title"
        onScroll={handleScroll}
      >
        <table id="ranking">
          <caption className="sr-only">
            Classement et statistiques des équipes. Le numéro reste le rang officiel.
          </caption>
          <RankingHeaders
            eloStat={eloStat}
            recordStat={recordStat}
            paceStat={paceStat}
            sort={sort}
            onSortChange={onSortChange}
          />
          {/* oxlint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- Highlighting delegates to the row's real focusable controls. */}
          <tbody
            onPointerOver={handlePointerOver}
            onFocus={showOpponent}
            onPointerOut={handlePointerOut}
            onBlur={clearOpponent}
          >
            {sorted.map((rating) => {
              const active = !rating.archived;
              const duel = active ? duels.get(rating.team) : undefined;
              return (
                <RankingRow
                  key={active ? rating.team : `${rating.team}@${rating.build}`}
                  rank={active ? ranks.get(rating.team) : undefined}
                  rating={rating}
                  version={versionOf(rating.team, rating.build)}
                  duel={duel}
                  eloStat={eloStat}
                  recordStat={recordStat}
                  paceStat={paceStat}
                  highlight={active && opponent === rating.team}
                />
              );
            })}
          </tbody>
        </table>
        <p id="ranking-empty" className="empty" hidden={!loaded || ratings.length > 0}>
          Aucune partie pour l&apos;instant : le classement se remplit dès que deux équipes ont un
          jar accepté.
        </p>
      </section>
      {rankingNotes(ratings, timeMs)}
    </section>
  );
};

export { RankingSection };
