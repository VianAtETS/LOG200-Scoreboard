import { useGetDuelPairs } from '@/dao/hooks/use-games';
import { record } from '@/lib/elo';
import { pageUrl } from '@/lib/url';
import type { TGameRow } from '@/types/games.types';
import type { TTeamRating } from '@/types/summary.types';
import type { FC } from 'react';

type Props = {
  teams: string[];
  // Parties entre les deux équipes seulement.
  games: TGameRow[];
  ratings: TTeamRating[];
  versionOf: (team: string, build?: string) => string;
};

// Avec deux équipes choisies : bilan de la version actuelle de chacune contre l'autre, et lien vers
// leur duel avec le nombre de paires complètes de ces versions.
const DuelRecord: FC<Props> = ({ teams, games, ratings, versionOf }) => {
  const current = (team: string) => ratings.find((r) => r.team === team)?.build;
  const sides = teams.length === 2 ? teams.map((team) => ({ team, build: current(team) })) : null;
  const { data: pairs } = useGetDuelPairs(sides);
  if (!sides) return <p id="duel" className="graph-link" hidden />;

  const line = (team: string, opponent: string) => {
    const build = current(team);
    const { wins, games: n } = record(games, team, opponent, build);
    const name = [team, versionOf(team, build)].filter(Boolean).join(' ');
    const score = n ? `${wins}–${n - wins} (${Math.round((100 * wins) / n)} %)` : 'aucune partie';
    return `${name} contre ${opponent} : ${score}`;
  };

  return (
    <p
      id="duel"
      className="graph-link"
      title="Version actuelle de chaque équipe, contre toutes les versions de l'autre"
    >
      {`${line(teams[0], teams[1])} · ${line(teams[1], teams[0])} · `}
      <a href={pageUrl('duel.html', { builds: sides.map((s) => s.build ?? '*').join(',') })}>
        {pairs ? `Duel : ${pairs.length} paire${pairs.length > 1 ? 's' : ''}` : 'Duel'}
      </a>
    </p>
  );
};

export { DuelRecord };
