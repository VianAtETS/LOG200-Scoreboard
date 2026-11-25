// Page graphe : rang ou Elo des équipes choisies après chaque partie, rejoué depuis
// timeline.jsonl ; un nœud marque chaque nouvelle version de jar.

import { BumpChart } from '@/components/bump/bump-chart';
import { colorOf } from '@/components/bump/bump.utils';
import { LiveStatus } from '@/components/controls/live-status';
import { SessionSelect, sessionOptions } from '@/components/controls/session-select';
import { VerifiedFilter } from '@/components/controls/verified-filter';
import { WindowRadio } from '@/components/controls/window-radio';
import { Masthead } from '@/components/masthead/masthead';
import { TeamPicker } from '@/components/picker/team-picker';
import { useGetTimeline } from '@/dao/hooks/use-games';
import { useGetSummary } from '@/dao/hooks/use-ladder';
import { usePulse } from '@/hooks/use-pulse';
import { useSearchParam } from '@/hooks/use-search-param';
import { rankSeries, topTeams } from '@/lib/bump';
import { GLOBAL, WINDOW_OPTIONS, WINDOWS, windowOf } from '@/lib/constants';
import { timeFormat, versionOf } from '@/lib/format';
import { withProviders } from '@/providers/with-providers';
import { useCallback, useMemo } from 'react';

const CHART_MODES = [
  { value: 'rank', label: 'Rang' },
  { value: 'rating', label: 'Elo' },
];

const GraphePage = () => {
  const { data: summary, error, dataUpdatedAt } = useGetSummary();
  const {
    data: timeline,
    error: timelineError,
    dataUpdatedAt: timelineUpdatedAt,
  } = useGetTimeline();

  const [sessionChoice, setSessionChoice] = useSearchParam('session', true);
  const [verifiedParam, setVerifiedParam] = useSearchParam('verified');
  const [windowParam, setWindowParam] = useSearchParam('window');
  const [metricParam, setMetricParam] = useSearchParam('metric');
  // Absent : le top 10, qui suit le classement ; vide : aucune équipe.
  const [teamsParam, setTeamsParam] = useSearchParam('teams', true);
  const window = windowOf(windowParam);
  const metric = metricParam === 'elo' ? 'rating' : 'rank';

  // Sans ?session=, la session la plus récente.
  const sessions = summary?.sessions ?? [];
  const wanted = sessionChoice ?? sessions[0];
  const session = sessions.includes(wanted) ? wanted : GLOBAL;
  const verified = Boolean(summary?.verified) && verifiedParam === '1';
  const scope = summary ? (verified ? summary.verified! : summary.scopes)[session] : undefined;
  const ratings = useMemo(() => scope?.ratings ?? [], [scope]);
  const builds = useMemo(() => new Map(Object.entries(summary?.builds ?? {})), [summary]);
  const version = useCallback(
    (team: string, build?: string) => versionOf(builds, team, build),
    [builds],
  );

  const selected = useMemo(() => {
    if (teamsParam === null) return topTeams(ratings);
    const known = new Set(ratings.map((r) => r.team));
    return teamsParam.split(',').filter((t) => t && known.has(t));
  }, [teamsParam, ratings]);

  const cutoff = dataUpdatedAt - (WINDOWS[window] ?? Infinity);
  const { columns, series } = useMemo(() => {
    const rows =
      session === GLOBAL ? (timeline ?? []).filter((t) => !t.tournament) : (timeline ?? []);
    return rankSeries(rows, {
      key: session === GLOBAL ? 'all' : 'session',
      session,
      selected,
      cutoff,
      verified,
    });
  }, [timeline, session, selected, cutoff, verified]);

  // Un bot reste gris et ne prend pas de couleur.
  const palette = useMemo(() => {
    const bots = new Set(ratings.filter((r) => !r.build).map((r) => r.team));
    return new Map(selected.filter((t) => !bots.has(t)).map((t) => [t, colorOf(t, selected)]));
  }, [ratings, selected]);

  const pulses = usePulse(summary) + usePulse(timeline);

  const failure = error ?? timelineError;
  const status = failure
    ? `Données indisponibles : ${failure.message}`
    : summary && timeline
      ? `${columns.length} parties dans la fenêtre, à jour à ${timeFormat.format(new Date(Math.max(dataUpdatedAt, timelineUpdatedAt)))}`
      : 'Chargement des parties…';

  // null : retour au haut du classement (?teams= retiré) ; [] : aucune équipe (?teams= vide).
  const choose = (teams: string[] | null) => setTeamsParam(teams === null ? null : teams.join(','));

  return (
    <>
      <Masthead
        current="graphe"
        status={
          <LiveStatus
            dot={failure ? 'error' : summary && timeline ? 'on' : ''}
            text={status}
            pulses={pulses}
          />
        }
      >
        <VerifiedFilter
          hidden={!summary?.verified}
          checked={verifiedParam === '1'}
          onChange={(checked) => setVerifiedParam(checked ? '1' : null)}
        />
        <SessionSelect
          value={session}
          options={sessionOptions(summary?.sessions)}
          onChange={setSessionChoice}
        />
      </Masthead>

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
              onChange={(value) => setMetricParam(value === 'rating' ? 'elo' : null)}
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
              color={(team) => palette.get(team) ?? 'var(--color-text-muted)'}
              idle={(team) => !series.get(team)?.length}
            />
            <button
              id="picker-top"
              className="picker-top"
              type="button"
              onClick={() => choose(null)}
            >
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
            loading={!failure && !(summary && timeline)}
          />
        </section>
      </main>
    </>
  );
};

export default withProviders(GraphePage);
