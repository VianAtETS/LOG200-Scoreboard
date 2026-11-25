// Page duel : deux équipes (?teams=a,b) jugées sur leurs paires complètes du ladder, même ouverture et
// couleurs inversées (lib/duel.ts). ?builds=h1,h2 choisit un build par équipe, `*` toutes ses versions ;
// sans lui, les builds actuels.

import { Masthead } from '@/components/masthead/masthead';
import { useGetDuelPairs } from '@/dao/hooks/use-games';
import { useGetSummary } from '@/dao/hooks/use-ladder';
import { useSearchParam } from '@/hooks/use-search-param';
import { pairCounts } from '@/lib/duel';
import { versionOf } from '@/lib/format';
import { pageUrl } from '@/lib/url';
import { withProviders } from '@/providers/with-providers';
import { useCallback, useMemo } from 'react';
import type { JSX } from 'react';
import { BuildSelectors, DuelOutcome, PairTable, useDuelChoices } from './duel-results';

const ALL = '*';

const choice = (requested: string | undefined, fallback: string | undefined): string => {
  if (requested !== undefined && requested !== '') {
    return requested;
  }
  return fallback === undefined || fallback === '' ? ALL : fallback;
};

const duelSides = (
  teams: string[],
  chosen: string[],
  ready: boolean,
): { team: string; build?: string }[] | null => {
  if (teams.length !== 2 || !ready) {
    return null;
  }
  return teams.map((team, index) => ({
    team,
    ...(chosen[index] === ALL || chosen[index] === undefined ? {} : { build: chosen[index] }),
  }));
};

const displayName = (team: string, build: string, builds: Map<string, string[]>): string =>
  `${team} ${build === ALL ? '(toutes versions)' : versionOf(builds, team, build)}`.trim();
const DuelPageContent = (): JSX.Element => {
  const { data: summary, error: summaryError } = useGetSummary();
  const [teamsParam] = useSearchParam('teams');
  const [buildsParam, setBuildsParam] = useSearchParam('builds');
  const teams = useMemo(() => (teamsParam ?? '').split(',').filter(Boolean), [teamsParam]);
  const builds = useMemo(() => new Map(Object.entries(summary?.builds ?? {})), [summary]);
  const current = useCallback(
    (team: string): string | undefined =>
      summary?.scopes['']?.ratings.find((rating) => rating.team === team)?.build,
    [summary],
  );

  // Le build choisi de chaque équipe, ALL pour toutes ses versions.
  const chosen = useMemo(
    () =>
      teams.map((team, index) => {
        const requested = buildsParam?.split(',')[index];
        const fallback = current(team);
        return choice(requested, fallback);
      }),
    [teams, buildsParam, current],
  );
  const firstChoice = chosen[0] ?? ALL;
  const secondChoice = chosen[1] ?? ALL;
  const sides = duelSides(teams, chosen, summary !== undefined);
  const { data: pairs, error: pairsError } = useGetDuelPairs(sides);

  const { choose, showAll } = useDuelChoices(firstChoice, secondChoice, setBuildsParam);

  if (teams.length !== 2) {
    return (
      <>
        <Masthead />
        <main className="layout duel-layout">
          <section className="section">
            <h2>Duel</h2>
            <p>
              Choisissez deux équipes dans le <a href={pageUrl('./')}>Classement</a>, puis ouvrez
              leur duel depuis la ligne de face-à-face.
            </p>
          </section>
        </main>
      </>
    );
  }

  const failure = summaryError ?? pairsError;
  const counts = pairs && teams[0] !== undefined ? pairCounts(pairs, teams[0]) : null;

  return (
    <>
      <Masthead />
      <main className="layout duel-layout">
        <section className="section duel-head" aria-labelledby="duel-title">
          <h2 id="duel-title">
            Duel : {teams[0]} contre {teams[1]}
          </h2>
          <BuildSelectors
            teams={teams}
            chosen={chosen}
            builds={builds}
            current={current}
            onChange={choose}
          />
          <DuelOutcome
            failure={failure}
            pairs={pairs}
            counts={counts}
            firstName={displayName(teams[0] ?? '', firstChoice, builds)}
            secondName={displayName(teams[1] ?? '', secondChoice, builds)}
            allVersions={chosen.every((build) => build === ALL)}
            onAllVersions={showAll}
          />
        </section>

        {pairs && pairs.length > 0 && <PairTable pairs={pairs} />}
      </main>
    </>
  );
};

export const DuelPage = withProviders(DuelPageContent);
