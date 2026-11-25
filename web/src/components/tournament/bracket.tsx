import type { TRunningGame } from '@/types/games.types';
import type { TModel } from '@/types/summary.types';
import type { TMatch, TTournament } from '@/types/tournament.types';
import { useMemo } from 'react';
import type { FC } from 'react';
import { BracketSection } from './bracket-section';
import { MatchCard } from './match-card';
import { SIDES, roundTitle } from './tournament.utils';

type Props = {
  view: TTournament;
  model: TModel | null | undefined;
  running: TRunningGame[];
  describe: (team: string) => string;
};

// Tableau à double élimination : le principal (avec la grande finale), puis le repêchage.
const Bracket: FC<Props> = ({ view, model, running, describe }) => {
  const seeds = new Map(view.entrants.map((entrant, index) => [entrant.team, index + 1]));
  const byId = new Map(view.matches.map((match) => [match.id, match]));

  const sections = useMemo(
    () =>
      SIDES.map(([side, title]) => {
        const matches = view.matches.filter((match) => match.side === side);
        const last = Math.max(...matches.map((match) => match.round));
        const rounds: [string, TMatch[]][] = [];
        for (let round = 1; round <= last; round += 1) {
          rounds.push([
            roundTitle(side, round, last),
            matches.filter((match) => match.round === round),
          ]);
        }
        if (side === 'winners') {
          rounds.push([
            roundTitle('final'),
            view.matches.filter((match) => match.side === 'final'),
          ]);
        }
        return { side, title, rounds };
      }),
    [view.matches],
  );

  return (
    <div id="bracket" className="bracket">
      {sections.map(({ side, title, rounds }) => (
        <BracketSection key={side} side={side} title={title} rounds={rounds}>
          {(list) =>
            list.map((match) => (
              <MatchCard
                key={match.id}
                match={match}
                view={view}
                seeds={seeds}
                byId={byId}
                model={model}
                running={running}
                describe={describe}
              />
            ))
          }
        </BracketSection>
      ))}
    </div>
  );
};

export { Bracket };
