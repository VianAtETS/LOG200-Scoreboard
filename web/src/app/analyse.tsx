// Page analyse : une partie rejouée coup par coup avec la mobilité, le territoire et la région de chaque
// couleur, ses coups critiques et son point de bascule (lib/analysis.ts). Sans ?game=, la dernière partie.

import {
  AnalysisStats,
  moveData,
  renderBoard,
  renderCharts,
  renderMoves,
  useAnalysisNavigation,
  useAnalysisSources,
} from './analyse-parts';
import { Masthead } from '@/components/masthead/masthead';
import { GameInfo } from '@/components/replay/replay-section';
import { analyse } from '@/lib/analysis';
import { versionOf } from '@/lib/format';
import { positions } from '@/lib/replay';
import { withProviders } from '@/providers/with-providers';
import type { TColor } from '@/types/games.types';
import { useCallback, useMemo } from 'react';
import type { ReactNode } from 'react';

const AnalysePageContent = (): React.JSX.Element => {
  const { summaryQuery, gameQuery, gameParam, row } = useAnalysisSources();
  const { data: summary, error: summaryError } = summaryQuery;
  const { data: game, error: gameError } = gameQuery;
  const builds = useMemo(() => new Map(Object.entries(summary?.builds ?? {})), [summary]);
  const version = useCallback(
    (team: string, build?: string) => versionOf(builds, team, build),
    [builds],
  );

  const steps = useMemo(() => (game ? positions(game) : []), [game]);
  const analysis = useMemo(() => (game ? analyse(steps, game) : null), [game, steps]);

  const crit = useMemo(
    () => [...(analysis?.crit.keys() ?? [])].toSorted((left, right) => left - right),
    [analysis],
  );
  const { last, k, go, prevCrit, nextCrit } = useAnalysisNavigation(steps.length, crit);

  const { timed, depths, rates, moverOf } = useMemo(() => moveData(game, steps), [game, steps]);

  const failure = gameError ?? (gameParam !== null && gameParam !== '' ? null : summaryError);
  const step = steps[k];
  const name = useCallback(
    (color: TColor): string =>
      game ? `${game[color].team} (${color === 'red' ? 'rouge' : 'noir'})` : '',
    [game],
  );
  let replayInfo: ReactNode = 'Aucune partie à analyser.';
  if (failure) {
    replayInfo = `Partie indisponible : ${failure.message}`;
  } else if (game) {
    replayInfo = <GameInfo game={game} versionOf={version} />;
  } else if (row || !summary) {
    replayInfo = 'Chargement de la partie…';
  }

  return (
    <>
      <Masthead current="analyse" />
      <main className="layout analyse-layout">
        <section className="section analyse-head" aria-labelledby="analyse-title">
          <h2 id="analyse-title">Analyse</h2>
          <p className="replay-info">{replayInfo}</p>
        </section>

        {game && analysis && step && (
          <>
            {renderBoard({ step, k, last, prevCrit, nextCrit, go, analysis, name })}
            {renderMoves({ steps, k, go, analysis, moverOf })}
            {renderCharts({ analysis, game, timed, depths, rates, last, k, crit, go, name })}
            <AnalysisStats
              analysis={analysis}
              timed={timed}
              depths={depths}
              rates={rates}
              name={name}
            />
          </>
        )}
      </main>
    </>
  );
};

export const AnalysePage = withProviders(AnalysePageContent);
