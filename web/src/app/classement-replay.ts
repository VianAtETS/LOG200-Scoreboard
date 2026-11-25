import { GLOBAL } from '@/lib/constants';
import { BETWEEN } from '@/lib/elo';
import { setParam } from '@/lib/url';
import { useReplay } from '@/components/replay/use-replay';
import { teamUrl } from '@/dao/scoreboard.repository';
import { useLiveGames } from '@/hooks/use-live-games';
import { pickLive } from '@/lib/live';
import { tail, tailState } from '@/lib/tail';
import type { TGameRow, TRunningGame } from '@/types/games.types';
import type { TSummary } from '@/types/summary.types';
// oxlint-disable-next-line import/max-dependencies -- Replay orchestration joins live updates, navigation and replay state.
import { useCallback, useEffect, useRef, useState } from 'react';

type TInput = {
  legacyTeam: boolean;
  linkedGame: string | null;
  summary: TSummary | undefined;
  dataUpdatedAt: number;
  teams: string[];
  teamsKey: string;
  list: TGameRow[];
  running: TRunningGame[];
  runningFetched: boolean;
  match: string;
};

export function useClassementReplay({
  legacyTeam,
  linkedGame,
  summary,
  dataUpdatedAt,
  teams,
  teamsKey,
  list,
  running,
  runningFetched,
  match,
}: TInput): ReturnType<typeof useReplay> {
  const { replay, actions } = useReplay();
  const [liveWanted, setLiveWanted] = useState(false);
  const live = useLiveGames(liveWanted);
  // ?team= des anciens liens devient ?teams=.
  const legacy = useRef(legacyTeam);
  useEffect(() => {
    if (!summary || !legacy.current) {
      return;
    }
    legacy.current = false;
    setParam('team', null);
    setParam('teams', teamsKey);
  }, [summary, teamsKey]);
  // Le direct ne s'ouvre que s'il sert : partie liée en cours, ou équipe choisie en partie. Une fois ouvert,
  // il le reste.
  const selectedTeams = new Set(teams);
  const linkedRunning = Boolean(linkedGame) && running.some((game) => game.id === linkedGame);
  const teamPlaying = running.some(
    (game) => Boolean(game.id) && (selectedTeams.has(game.red) || selectedTeams.has(game.black)),
  );
  if (!liveWanted && summary && runningFetched && (linkedRunning || teamPlaying)) {
    setLiveWanted(true);
  }
  // ?game= d'une partie terminée : retrouvée dans le fichier de l'équipe rouge.
  const linked = useRef(false);
  useEffect(() => {
    if (linked.current || !summary || !runningFetched || linkedGame === null || linkedGame === '') {
      return;
    }
    linked.current = true;
    if (running.some((game) => game.id === linkedGame)) {
      return;
    }
    const pair = linkedGame.slice(linkedGame.indexOf('-') + 1);
    const all = summary.scopes[GLOBAL]?.ratings.map((row) => row.team) ?? [];
    const teamNames = new Set(all);
    const red = all.find(
      (team) => pair.startsWith(`${team}-`) && teamNames.has(pair.slice(team.length + 1)),
    );
    const state = tailState<TGameRow>();
    const loadLinked = async (): Promise<void> => {
      if (red !== undefined && red !== '') {
        await tail(teamUrl(red), state);
      }
      const row = state.rows.find((game) => game.id === linkedGame);
      if (row) {
        await actions.showGame(row, true);
      } else {
        actions.unpin();
      }
    };
    // Effet React : pas de promesse retournée, les échecs libèrent la partie figée.
    void (async () => {
      try {
        await loadLinked();
      } catch {
        actions.unpin();
      }
    })();
  }, [summary, running, runningFetched, linkedGame, actions]);
  // Partie du direct à montrer : celle figée, sinon la plus récente des équipes choisies.
  const followLive = useCallback(() => {
    if (!liveWanted) {
      return false;
    }
    const current = actions.current();
    const game = current.pinned
      ? live.games.get(current.id ?? linkedGame ?? '')
      : pickLive(live.games, teams, match === BETWEEN, current.live?.id);
    if (game) {
      actions.showLive(game);
    }
    return Boolean(game);
  }, [liveWanted, live.games, teams, match, linkedGame, actions]);
  useEffect(() => {
    if (!liveWanted) {
      return;
    }
    if (
      !followLive() &&
      Boolean(linkedGame) &&
      (actions.current().id === null || actions.current().id === '') &&
      live.state === 'unavailable'
    ) {
      actions.setMessage('Direct indisponible sur ce serveur.');
    }
  }, [live, liveWanted, followLive, linkedGame, actions]);
  // Sans partie figée, le replay suit la dernière jouée.
  useEffect(() => {
    if (!summary) {
      return;
    }
    const current = actions.current();
    const latest = list.at(-1);
    const newer =
      latest && !(current.live && Boolean(current.id) && latest.id <= (current.id ?? ''));
    // Effet React : pas de promesse retournée, `showGame` traite ses propres échecs.
    if (!followLive() && newer && !current.pinned && current.id !== latest.id) {
      void actions.showGame(latest);
    }
    // oxlint-disable-next-line react/exhaustive-effect-dependencies -- Refetch completion and pin changes each require rechecking actions.current().
  }, [summary, dataUpdatedAt, list, replay.pinned, followLive, actions]);
  return { replay, actions };
}
