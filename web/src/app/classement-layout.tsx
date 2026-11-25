import { QueueDialog } from '@/components/queue/queue-dialog';
import { ArchiveFilter } from '@/components/ranking/archive-filter';
import { RankingSection } from '@/components/ranking/ranking-section';
import { ReplaySection } from '@/components/replay/replay-section';
import { SplitHandle } from '@/components/split-handle/split-handle';
import { SubmitSection } from '@/components/submit/submit-section';
import type { useGetModel, useGetRunning } from '@/dao/hooks/use-ladder';
import type { useClassementData } from './classement-data';
import { ClassementHeader } from './classement-header';
import { RecentGames } from './classement-games';
// oxlint-disable-next-line import/max-dependencies -- The page layout composes its section and state modules.
import type { useClassementReplay } from './classement-replay';
import type {
  useClassementLayout,
  useClassementSettings,
  useReplayFilters,
} from './classement-settings';

type Props = {
  data: ReturnType<typeof useClassementData>;
  replayState: ReturnType<typeof useClassementReplay>;
  settings: ReturnType<typeof useClassementSettings>;
  layout: ReturnType<typeof useClassementLayout>;
  filters: ReturnType<typeof useReplayFilters>;
  running: ReturnType<typeof useGetRunning>['data'];
  model: ReturnType<typeof useGetModel>['data'];
  verifiedParam: string | null;
  match: string;
  sideOrder: 'left' | 'right' | 'color';
};

export function ClassementLayout({
  data,
  replayState,
  settings,
  layout,
  filters,
  running,
  model,
  verifiedParam,
  match,
  sideOrder,
}: Props): React.JSX.Element {
  const {
    summary,
    scope,
    version,
    rankingRows,
    archiveChoices,
    wantedArchives,
    archiveError,
    chooseArchives,
  } = data;
  const { replay, actions } = replayState;
  const { eloStat, recordStat, paceStat, sort, changeElo, changeRecord, changePace, changeSort } =
    settings;
  const { queueOpen, collapsed, toggleRanking, toggleGames, openQueue, closeQueue } = layout;
  const { chooseTeams, changeSession, changeVerified, changeMatch, changeSide } = filters;
  const time = scope?.recent.at(-1)?.time_ms;
  return (
    <>
      <ClassementHeader
        data={data}
        time={time}
        verifiedParam={verifiedParam}
        changeVerified={changeVerified}
        changeSession={changeSession}
      />
      <main className="layout ranking-layout" data-collapsed={collapsed ?? undefined}>
        <RankingSection
          ratings={rankingRows}
          loaded={Boolean(scope)}
          running={running}
          model={model}
          versionOf={version}
          timeMs={time}
          eloStat={eloStat}
          recordStat={recordStat}
          paceStat={paceStat}
          sort={sort}
          onEloChange={changeElo}
          onRecordChange={changeRecord}
          onPaceChange={changePace}
          onSortChange={changeSort}
          onOpenQueue={openQueue}
          collapsed={collapsed === 'ranking'}
          onToggle={toggleRanking}
        >
          {summary?.archives && (
            <ArchiveFilter
              options={archiveChoices}
              wanted={wantedArchives}
              onChange={chooseArchives}
              error={archiveError?.message}
            />
          )}
        </RankingSection>
        <SplitHandle
          above="ranking-section"
          below="games-section"
          label="Partager la hauteur entre le classement et les parties récentes"
          storageKey="ranking-height"
        />
        <div className="replay-column">
          <SubmitSection />
          <ReplaySection replay={replay} actions={actions} versionOf={version} />
        </div>
        <RecentGames
          data={data}
          replay={replay}
          actions={actions}
          sideOrder={sideOrder}
          match={match}
          collapsed={collapsed === 'games'}
          onToggle={toggleGames}
          chooseTeams={chooseTeams}
          changeMatch={changeMatch}
          changeSide={changeSide}
        />
      </main>
      <QueueDialog open={queueOpen} onClose={closeQueue} running={running} />
    </>
  );
}
