// Page duel : deux équipes (?teams=a,b) jugées sur leurs paires complètes du ladder, même ouverture et
// couleurs inversées (lib/duel.ts). ?builds=h1,h2 choisit un build par équipe, `*` toutes ses versions ;
// sans lui, les builds actuels.

import { Masthead } from '@/components/masthead/masthead';
import { useGetDuelPairs } from '@/dao/hooks/use-games';
import { useGetSummary } from '@/dao/hooks/use-ladder';
import { useSearchParam } from '@/hooks/use-search-param';
import { SPRT_EXPECTED, eloGap, pairCounts, signTest, sprt, wilson } from '@/lib/duel';
import { signed, versionOf } from '@/lib/format';
import { pageUrl } from '@/lib/url';
import { withProviders } from '@/providers/with-providers';
import type { TDuelSide } from '@/types/games.types';
import { useMemo } from 'react';

const ALL = '*';
const percent = (v: number) => `${Math.round(100 * v)} %`;
const plural = (n: number, word: string) => `${n} ${word}${n > 1 ? 's' : ''}`;

const DuelPage = () => {
  const { data: summary, error: summaryError } = useGetSummary();
  const [teamsParam] = useSearchParam('teams');
  const [buildsParam, setBuildsParam] = useSearchParam('builds');
  const teams = (teamsParam ?? '').split(',').filter(Boolean);
  const builds = useMemo(() => new Map(Object.entries(summary?.builds ?? {})), [summary]);
  const current = (team: string) =>
    summary?.scopes['']?.ratings.find((r) => r.team === team)?.build;

  // Le build choisi de chaque équipe, ALL pour toutes ses versions.
  const chosen = teams.map((team, i) => buildsParam?.split(',')[i] || current(team) || ALL);
  const sides: TDuelSide[] | null =
    teams.length === 2 && summary
      ? teams.map((team, i) => ({ team, build: chosen[i] === ALL ? undefined : chosen[i] }))
      : null;
  const { data: pairs, error: pairsError } = useGetDuelPairs(sides);

  const choose = (i: number, build: string) =>
    setBuildsParam(chosen.map((b, j) => (j === i ? build : b)).join(','));
  const name = (i: number) => {
    const build = chosen[i] === ALL ? undefined : chosen[i];
    return `${teams[i]} ${build ? versionOf(builds, teams[i], build) : '(toutes versions)'}`.trim();
  };

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
  const counts = pairs ? pairCounts(pairs, teams[0]) : null;

  return (
    <>
      <Masthead />
      <main className="layout duel-layout">
        <section className="section duel-head" aria-labelledby="duel-title">
          <h2 id="duel-title">
            Duel : {teams[0]} contre {teams[1]}
          </h2>
          <div className="duel-builds">
            {teams.map((team, i) => {
              const own = builds.get(team) ?? [];
              return (
                <label key={team}>
                  <span>Version de {team}</span>
                  <select
                    value={chosen[i]}
                    onChange={(e) => choose(i, e.target.value)}
                    disabled={own.length === 0}
                  >
                    {own.map((build, n) => (
                      <option key={build} value={build}>
                        {`v${n + 1} (${build})${build === current(team) ? ', actuelle' : ''}`}
                      </option>
                    ))}
                    <option value={ALL}>Toutes les versions</option>
                  </select>
                </label>
              );
            })}
          </div>
          {failure ? (
            <p>Duel indisponible : {failure.message}</p>
          ) : !pairs || !counts ? (
            <p>Chargement des paires…</p>
          ) : (
            <Verdict
              names={[name(0), name(1)]}
              counts={counts}
              pairs={pairs.length}
              allVersions={chosen.every((b) => b === ALL)}
              onAllVersions={() => setBuildsParam(`${ALL},${ALL}`)}
            />
          )}
        </section>

        {pairs && pairs.length > 0 && (
          <section className="section duel-pairs" aria-labelledby="pairs-title">
            <h2 id="pairs-title">Paires, la plus récente d'abord</h2>
            <div className="analyse-scroll">
              <table className="analyse-table">
                <thead>
                  <tr>
                    <th scope="col">Ouverture</th>
                    <th scope="col">Première partie</th>
                    <th scope="col">Seconde partie</th>
                  </tr>
                </thead>
                <tbody>
                  {pairs.map((pair) => (
                    <tr key={pair.games[0]}>
                      <td>{pair.opening.join(' · ')}</td>
                      {pair.games.map((id, g) => (
                        <td key={id}>
                          <a href={pageUrl('analyse.html', { game: id })}>
                            {pair.winners[g]} gagne
                          </a>
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        )}
      </main>
    </>
  );
};

type VerdictProps = {
  names: [string, string];
  counts: ReturnType<typeof pairCounts>;
  pairs: number;
  allVersions: boolean;
  onAllVersions: () => void;
};

// Verdict du SPRT, puis le détail d'isola : paires, test du signe, score et Elo de la première équipe.
const Verdict = ({ names, counts, pairs, allVersions, onAllVersions }: VerdictProps) => {
  const decisive = counts.a + counts.b;
  const verdict = sprt(counts.a, counts.b);
  const p = signTest(counts.a, counts.b);
  const games = 2 * pairs;
  const wins = 2 * counts.a + counts.split;
  const score = games ? wins / games : Number.NaN;
  const [low, high] = wilson(wins, games);
  const elo = eloGap(score);

  return (
    <>
      <p className="duel-verdict">
        {verdict === 'a' && `${names[0]} est meilleur.`}
        {verdict === 'b' && `${names[1]} est meilleur.`}
        {verdict === 'close' && 'Pas de grand écart entre les deux.'}
        {verdict === 'undecided' &&
          `Indécis : ${plural(decisive, 'paire')} ${decisive > 1 ? 'décisives' : 'décisive'}, environ ${SPRT_EXPECTED} nécessaires.`}
        {verdict === 'undecided' && !allVersions && (
          <button type="button" className="share" onClick={onAllVersions}>
            Toutes les versions
          </button>
        )}
      </p>
      <ul className="duel-detail">
        <li>
          {plural(pairs, 'paire')} : {counts.a} gagnées 2-0 par {names[0]}, {counts.b} par{' '}
          {names[1]}, {counts.split} à 1-1 (décidées par l'ouverture)
        </li>
        {games > 0 && (
          <li>
            Score de {names[0]} : {percent(score)} des parties (IC 95 % : {percent(low)} à{' '}
            {percent(high)}), écart Elo {Number.isFinite(elo) ? signed(Math.round(elo)) : '–'}
          </li>
        )}
        {decisive > 0 && (
          <li>
            Test du signe sur les paires 2-0 : p{' '}
            {p < 0.001 ? '< 0,001' : `= ${p.toLocaleString('fr-CA', { maximumFractionDigits: 3 })}`}
          </li>
        )}
      </ul>
    </>
  );
};

export default withProviders(DuelPage);
