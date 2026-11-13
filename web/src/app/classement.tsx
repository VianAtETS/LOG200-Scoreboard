// Page du ladder : classement et dernières parties de summary.json, parties des équipes choisies
// dans teams/, replay lu partie par partie sur /api/games/<id>.
// ?session=, ?teams=, ?match=, ?window=, ?verified=, ?pace=, ?record=, ?archives= et ?game= suivent les choix de la page : l'adresse
// se partage telle quelle. ?team= des anciens liens vaut ?teams= avec cette seule équipe.

import { LiveStatus } from '@/components/controls/live-status';
import { SessionSelect, sessionOptions } from '@/components/controls/session-select';
import { VerifiedFilter } from '@/components/controls/verified-filter';
import { WindowRadio } from '@/components/controls/window-radio';
import { DuelRecord } from '@/components/games/duel-record';
import { GamesList } from '@/components/games/games-list';
import { EloChart } from '@/components/elo-chart/elo-chart';
import { Masthead } from '@/components/masthead/masthead';
import { TeamPicker } from '@/components/picker/team-picker';
import { QueueDialog } from '@/components/queue/queue-dialog';
import { ArchiveFilter } from '@/components/ranking/archive-filter';
import { RankingPanel } from '@/components/ranking/ranking-panel';
import { DEFAULT_STATS, ELO_STATS, nextStat, PACE_STATS, RECORD_STATS, statOf } from '@/components/ranking/ranking.data';
import { ReplayPanel } from '@/components/replay/replay-panel';
import { useReplay } from '@/components/replay/use-replay';
import { SubmitPanel } from '@/components/submit/submit-panel';
import { useGetTeamRows } from '@/dao/hooks/use-games';
import { useGetModel, useGetRunning, useGetSummary } from '@/dao/hooks/use-ladder';
import { teamUrl } from '@/dao/scoreboard.repository';
import { useLiveGames } from '@/hooks/use-live-games';
import { usePulse } from '@/hooks/use-pulse';
import { useSearchParam } from '@/hooks/use-search-param';
import { archivedRows, archiveOptions, withArchivedRows } from '@/lib/archives';
import { GLOBAL, windowOf } from '@/lib/constants';
import { BETWEEN, gamesOf, isRemote, ratingOf } from '@/lib/elo';
import { timeFormat, versionOf } from '@/lib/format';
import { pickLive } from '@/lib/live';
import { tail, tailState } from '@/lib/tail';
import { pageUrl, searchParams, setParam } from '@/lib/url';
import { withProviders } from '@/providers/with-providers';
import type { TGameRow, TSide } from '@/types/games.types';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

const MATCH_OPTIONS = [
  { value: GLOBAL, label: "Avec l'une" },
  { value: BETWEEN, label: 'Entre elles' },
];

const outside = (g: TGameRow, session: string) => session === GLOBAL && Boolean(g.tournament);

const ClassementPage = () => {
  const { data: summary, error, dataUpdatedAt } = useGetSummary();
  const { data: running = [], isFetched: runningFetched } = useGetRunning();
  const { data: model } = useGetModel();
  const { replay, actions } = useReplay();

  // Valeurs de l'adresse au chargement.
  const [initial] = useState(searchParams);
  const linkedGame = initial.get('game');

  const [sessionChoice, setSessionChoice] = useSearchParam('session', true);
  const [verifiedParam, setVerifiedParam] = useSearchParam('verified');
  const [matchParam, setMatchParam] = useSearchParam('match');
  const [windowParam, setWindowParam] = useSearchParam('window');
  const [paceParam, setPaceParam] = useSearchParam('pace');
  const [recordParam, setRecordParam] = useSearchParam('record');
  const [eloParam, setEloParam] = useSearchParam('elo');
  // Aucune équipe choisie : les dernières parties de toutes les équipes.
  const [chosen, setChosen] = useState(() => (initial.get('teams') ?? initial.get('team') ?? '').split(',').filter(Boolean));
  const [queueOpen, setQueueOpen] = useState(false);
  const [liveWanted, setLiveWanted] = useState(false);
  const live = useLiveGames(liveWanted);

  const paceStat = statOf(PACE_STATS, paceParam, DEFAULT_STATS.pace);
  const recordStat = statOf(RECORD_STATS, recordParam, DEFAULT_STATS.record);
  const eloStat = statOf(ELO_STATS, eloParam, DEFAULT_STATS.elo);
  const match = matchParam ?? GLOBAL;
  const window = windowOf(windowParam);

  // Sans ?session=, la session la plus récente.
  const sessions = summary?.sessions ?? [];
  const wanted = sessionChoice ?? sessions[0];
  const session = sessions.includes(wanted) ? wanted : GLOBAL;
  const key = session === GLOBAL ? 'all' : 'session';
  const verified = Boolean(summary?.verified) && verifiedParam === '1';
  const scope = summary ? (verified ? summary.verified! : summary.scopes)[session] : undefined;
  const ratings = scope?.ratings ?? [];
  const builds = useMemo(() => new Map(Object.entries(summary?.builds ?? {})), [summary]);
  const version = useCallback((team: string, build?: string) => versionOf(builds, team, build), [builds]);
  const rating = useCallback((side: TSide) => ratingOf(side, key, verified), [key, verified]);

  const known = useMemo(() => new Set(summary?.scopes[GLOBAL].ratings.map((r) => r.team)), [summary]);
  const teamsKey = summary ? chosen.filter((t) => known.has(t)).join(',') : '';
  const teams = useMemo(() => teamsKey.split(',').filter(Boolean), [teamsKey]);
  const { data: rows, isPlaceholderData } = useGetTeamRows(teams);

  // Versions archivées : seulement avec un ladder lancé avec --archives.
  const [archivesParam, setArchivesParam] = useSearchParam('archives');
  const archiveChoices = useMemo(() => (summary?.archives ? archiveOptions(summary) : []), [summary]);
  const wantedArchives = useMemo(() => (archivesParam ?? '').split(',').filter(Boolean), [archivesParam]);
  const pickedArchives = useMemo(
    () => archiveChoices.filter((o) => wantedArchives.includes(o.key)),
    [archiveChoices, wantedArchives]
  );
  const archiveTeamsKey = [...new Set(pickedArchives.map((o) => o.team))].join(',');
  const archiveTeams = useMemo(() => archiveTeamsKey.split(',').filter(Boolean), [archiveTeamsKey]);
  const { data: archiveFiles, error: archiveError } = useGetTeamRows(archiveTeams, 'archives');
  const rankingRows = useMemo(() => {
    const games = new Map([...(archiveFiles?.values() ?? [])].flat().map((g) => [g.id, g]));
    return withArchivedRows(scope?.ratings ?? [], archivedRows([...games.values()], pickedArchives, { session, verified }));
  }, [archiveFiles, pickedArchives, scope, session, verified]);

  const list = useMemo(() => {
    if (!scope) return [];
    if (!teams.length) return scope.recent;
    return gamesOf(rows ?? new Map(), teams, { mode: match, session }).filter(
      (g) => (!verified || !isRemote(g)) && !outside(g, session)
    );
  }, [scope, teams, rows, match, session, verified]);

  const duelGames = useMemo(
    () =>
      teams.length === 2
        ? gamesOf(rows ?? new Map(), teams, { mode: BETWEEN, session }).filter(
            (g) => (!verified || !isRemote(g)) && !outside(g, session)
          )
        : [],
    [teams, rows, session, verified]
  );

  // Le point du direct pulse quand des parties arrivent, pas au chargement ni à un changement d'équipes.
  const pulses = usePulse(summary) + usePulse(isPlaceholderData ? undefined : rows, teamsKey);

  // ?team= des anciens liens devient ?teams=.
  const legacy = useRef(initial.has('team'));
  useEffect(() => {
    if (!summary || !legacy.current) return;
    legacy.current = false;
    setParam('team', null);
    setParam('teams', teamsKey);
  }, [summary, teamsKey]);

  // Le direct ne s'ouvre que s'il sert : partie liée en cours, ou équipe choisie en partie. Une fois ouvert,
  // il le reste.
  const linkedRunning = Boolean(linkedGame) && running.some((g) => g.id === linkedGame);
  const teamPlaying = running.some((g) => g.id && (teams.includes(g.red) || teams.includes(g.black)));
  if (!liveWanted && summary && runningFetched && (linkedRunning || teamPlaying)) setLiveWanted(true);

  // ?game= d'une partie terminée : retrouvée dans le fichier de l'équipe rouge.
  const linked = useRef(false);
  useEffect(() => {
    if (linked.current || !summary || !runningFetched || !linkedGame) return;
    linked.current = true;
    if (running.some((g) => g.id === linkedGame)) return;
    const pair = linkedGame.slice(linkedGame.indexOf('-') + 1);
    const all = summary.scopes[GLOBAL].ratings.map((r) => r.team);
    const red = all.find((t) => pair.startsWith(`${t}-`) && all.includes(pair.slice(t.length + 1)));
    const state = tailState<TGameRow>();
    (red ? tail(teamUrl(red), state) : Promise.resolve(false))
      .then(() => {
        const row = state.rows.find((g) => g.id === linkedGame);
        if (row) actions.showGame(row, true);
        else actions.unpin();
      })
      .catch(() => actions.unpin());
  }, [summary, running, runningFetched, linkedGame, actions]);

  // Partie du direct à montrer : celle figée, sinon la plus récente des équipes choisies.
  const followLive = useCallback(() => {
    if (!liveWanted) return false;
    const r = actions.current();
    const shown = r.live && r.id ? live.games.get(r.id) : undefined;
    const g = r.pinned
      ? live.games.get(r.id ?? linkedGame ?? '')
      : teams.length
        ? (pickLive(live.games, teams, match === BETWEEN) ?? shown)
        : undefined;
    if (g) actions.showLive(g);
    return Boolean(g);
  }, [liveWanted, live.games, teams, match, linkedGame, actions]);

  useEffect(() => {
    if (!liveWanted) return;
    if (!followLive() && linkedGame && !actions.current().id && live.state === 'unavailable')
      actions.setMessage('Direct indisponible sur ce serveur.');
  }, [live, liveWanted, followLive, linkedGame, actions]);

  // Sans partie figée, le replay suit la dernière jouée.
  useEffect(() => {
    if (!summary) return;
    const r = actions.current();
    const latest = list.at(-1);
    const newer = latest && !(r.live && r.id && latest.id <= r.id);
    if (!followLive() && newer && !r.pinned && r.id !== latest.id) actions.showGame(latest);
  }, [summary, dataUpdatedAt, list, replay.pinned, followLive, actions]);

  const reselect = () => actions.unpin();

  const chooseTeams = (next: string[]) => {
    setChosen(next);
    setParam('teams', next.join(','));
    reselect();
  };

  const time = scope?.recent.at(-1)?.time_ms;
  const status = error
    ? `Données indisponibles : ${error.message}`
    : scope
      ? `${teams.length ? list.length : scope.games} parties${time ? `, ${time} ms par coup` : ''}, à jour à ${timeFormat.format(new Date(dataUpdatedAt))}`
      : 'Chargement des parties…';
  const chartTeam = teams.length === 1 ? ratings.find((r) => r.team === teams[0]) : undefined;

  return (
    <>
      <Masthead current="classement">
        <VerifiedFilter
          hidden={!summary?.verified}
          checked={verifiedParam === '1'}
          onChange={(checked) => {
            setVerifiedParam(checked ? '1' : null);
            reselect();
          }}
        />
        <SessionSelect
          value={session}
          options={sessionOptions(summary?.sessions)}
          onChange={(value) => {
            setSessionChoice(value);
            reselect();
          }}
        />
        <LiveStatus dot={error ? 'error' : summary ? 'on' : ''} text={status} pulses={pulses} />
      </Masthead>

      <main className="layout">
        <RankingPanel
          ratings={rankingRows}
          loaded={Boolean(scope)}
          running={running}
          model={model}
          versionOf={version}
          timeMs={time}
          eloStat={eloStat}
          recordStat={recordStat}
          paceStat={paceStat}
          onNextElo={() => setEloParam(eloStat === 'rating' ? 'peak' : null)}
          onNextRecord={() => setRecordParam(recordStat === 'all' ? 'build' : null)}
          onNextPace={() => {
            const next = nextStat(PACE_STATS, paceStat);
            setPaceParam(next === DEFAULT_STATS.pace ? null : next);
          }}
          onOpenQueue={() => setQueueOpen(true)}
        >
          {summary?.archives && (
            <ArchiveFilter
              options={archiveChoices}
              wanted={wantedArchives}
              onChange={(next) => setArchivesParam(next.join(',') || null)}
              error={archiveError?.message}
            />
          )}
        </RankingPanel>

        <SubmitPanel />

        <ReplayPanel replay={replay} actions={actions} versionOf={version} />

        <section className="panel games" aria-labelledby="games-title">
          <div className="games-head">
            <h2 id="games-title">Parties récentes</h2>
            <WindowRadio
              id="match"
              className="match"
              name="match"
              label="Parties montrées"
              value={match}
              options={MATCH_OPTIONS}
              hidden={teams.length < 2}
              onChange={(value) => {
                setMatchParam(value);
                reselect();
              }}
            />
          </div>
          <TeamPicker
            className="games-picker"
            ratings={ratings}
            selected={teams}
            onChange={chooseTeams}
            placeholder="Filtrer par équipe"
            chipsLabel="Équipes choisies"
          />
          <a
            id="graph-link"
            className="graph-link"
            hidden={teams.length < 2}
            href={pageUrl('graphe.html', { teams: teams.join(',') })}
          >
            Voir leurs rangs dans le graphe →
          </a>
          <DuelRecord teams={teams} games={duelGames} ratings={ratings} versionOf={version} />
          {chartTeam && (
            <EloChart
              team={chartTeam}
              rows={list}
              rating={rating}
              versionOf={version}
              window={window}
              now={dataUpdatedAt}
              onWindowChange={setWindowParam}
              replayId={replay.id}
              onOpen={(g) => actions.showGame(g, true)}
            />
          )}
          <GamesList games={list} rating={rating} currentId={replay.id} onSelect={(g) => actions.showGame(g, true)} />
        </section>
      </main>

      <QueueDialog open={queueOpen} onClose={() => setQueueOpen(false)} running={running} />
    </>
  );
};

export default withProviders(ClassementPage);
