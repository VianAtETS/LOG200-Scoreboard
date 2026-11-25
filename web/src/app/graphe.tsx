// Page graphe : rang ou Elo des équipes choisies après chaque partie, rejoué depuis
// timeline.jsonl ; un nœud marque chaque nouvelle version de jar.

import { BumpChart } from '@/components/bump/bump-chart';
import { WindowRadio } from '@/components/controls/window-radio';
import { TeamPicker } from '@/components/picker/team-picker';
import { useSearchParam } from '@/hooks/use-search-param';
import { WINDOWS, WINDOW_OPTIONS, windowOf } from '@/lib/constants';
import { withProviders } from '@/providers/with-providers';
import { useCallback } from 'react';
import type { JSX } from 'react';
import { useGraphData } from './graphe-data';
import { GraphHeader } from './graphe-header';

const CHART_MODES = [
  { value: 'rank', label: 'Rang' },
  { value: 'rating', label: 'Elo' },
];

const GraphePageContent = (): JSX.Element => {
  const [sessionChoice, setSessionChoice] = useSearchParam('session', true);
  const [verifiedParam, setVerifiedParam] = useSearchParam('verified');
  const [windowParam, setWindowParam] = useSearchParam('window');
  const [metricParam, setMetricParam] = useSearchParam('metric');
  // Absent : le top 10, qui suit le classement ; vide : aucune équipe.
  const [teamsParam, setTeamsParam] = useSearchParam('teams', true);
  const window = windowOf(windowParam);
  const duration = WINDOWS[window] ?? Infinity;
  const metric = metricParam === 'elo' ? 'rating' : 'rank';

  const {
    session,
    ratings,
    selected,
    columns,
    series,
    palette,
    version,
    pulses,
    sessions,
    status,
  } = useGraphData({ sessionChoice, verifiedParam, teamsParam, duration });

  // null : retour au classement ; [] : aucune équipe.
  const choose = useCallback(
    (teams: string[] | null): void => {
      setTeamsParam(teams === null ? null : teams.join(','));
    },
    [setTeamsParam],
  );

  const selectTopTeams = useCallback((): void => choose(null), [choose]);

  const setVerified = useCallback(
    (checked: boolean): void => {
      setVerifiedParam(checked ? '1' : null);
    },
    [setVerifiedParam],
  );

  const setMetric = useCallback(
    (value: string): void => {
      setMetricParam(value === 'rating' ? 'elo' : null);
    },
    [setMetricParam],
  );

  const color = useCallback(
    (team: string): string => palette.get(team) ?? 'var(--color-text-muted)',
    [palette],
  );

  const isIdle = useCallback(
    (team: string): boolean => (series.get(team)?.length ?? 0) === 0,
    [series],
  );

  return (
    <>
      <GraphHeader
        status={status}
        pulses={pulses}
        sessions={sessions}
        session={session}
        verified={verifiedParam === '1'}
        onVerified={setVerified}
        onSession={setSessionChoice}
      />

      <main className="layout graph-layout">
        <section className="section bump" aria-labelledby="bump-title">
          <div className="bump-head">
            <h2 id="bump-title">{metric === 'rank' ? 'Rangs' : 'Elo'} au fil des parties</h2>
            <WindowRadio
              id="chart-mode"
              name="metric"
              label="Mesure du graphe"
              value={metric}
              options={CHART_MODES}
              onChange={setMetric}
            />
            <WindowRadio
              id="bump-window"
              name="window"
              label="Fenêtre du graphe"
              value={window}
              options={WINDOW_OPTIONS}
              onChange={setWindowParam}
            />
          </div>
          <div className="bump-pick">
            <TeamPicker
              ratings={ratings}
              selected={selected}
              onChange={choose}
              placeholder="Ajouter une équipe"
              chipsLabel="Équipes suivies"
              color={color}
              idle={isIdle}
            />
            <button id="picker-top" className="picker-top" type="button" onClick={selectTopTeams}>
              Top 10
            </button>
          </div>
          <BumpChart
            columns={columns}
            series={series}
            selected={selected}
            metric={metric}
            palette={palette}
            versionOf={version}
            loading={status.dot === ''}
          />
        </section>
      </main>
    </>
  );
};

export const GraphePage = withProviders(GraphePageContent);
