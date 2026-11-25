// Page du ladder : classement et dernières parties de summary.json, parties des équipes choisies
// dans teams/, replay lu partie par partie sur /api/games/<id>.

import { useGetModel, useGetRunning } from '@/dao/hooks/use-ladder';
import { useSearchParam } from '@/hooks/use-search-param';
import { GLOBAL } from '@/lib/constants';
import { searchParams } from '@/lib/url';
import { withProviders } from '@/providers/with-providers';
import { useState } from 'react';
import { useClassementData } from './classement-data';
import { ClassementLayout } from './classement-layout';
import {
  useClassementLayout,
  useClassementSettings,
  useReplayFilters,
} from './classement-settings';
import { useClassementReplay } from './classement-replay';

const ClassementPageContent = (): React.JSX.Element => {
  const { data: running, isFetched: runningFetched } = useGetRunning();
  const { data: model } = useGetModel();

  // The query is captured once so replay links do not change as filters update.
  // oxlint-disable-next-line react/hook-use-state -- The initial URL is an immutable snapshot per mount.
  const [initial] = useState(searchParams);
  const linkedGame = initial.get('game');

  const [sessionChoice, setSessionChoice] = useSearchParam('session', true);
  const [verifiedParam, setVerifiedParam] = useSearchParam('verified');
  const [matchParam, setMatchParam] = useSearchParam('match');
  const [sideParam, setSideParam] = useSearchParam('side');

  // Aucune équipe choisie : les dernières parties de toutes les équipes.
  const [chosen, setChosen] = useState(() =>
    (initial.get('teams') ?? initial.get('team') ?? '').split(',').filter(Boolean),
  );

  const layout = useClassementLayout();

  const settings = useClassementSettings();

  const match = matchParam ?? GLOBAL;
  const sideOrder = sideParam === 'left' || sideParam === 'right' ? sideParam : 'color';

  const data = useClassementData({ sessionChoice, verifiedParam, match, chosen });
  const { summary, dataUpdatedAt, teams, teamsKey, list } = data;
  const replayState = useClassementReplay({
    legacyTeam: initial.has('team'),
    linkedGame,
    summary,
    dataUpdatedAt,
    teams,
    teamsKey,
    list,
    running,
    runningFetched,
    match,
  });
  const filters = useReplayFilters({
    setChosen,
    setSessionChoice,
    setVerifiedParam,
    setMatchParam,
    setSideParam,
    unpin: replayState.actions.unpin,
  });

  return (
    <ClassementLayout
      data={data}
      replayState={replayState}
      settings={settings}
      layout={layout}
      filters={filters}
      running={running}
      model={model}
      verifiedParam={verifiedParam}
      match={match}
      sideOrder={sideOrder}
    />
  );
};

export const ClassementPage = withProviders(ClassementPageContent);
