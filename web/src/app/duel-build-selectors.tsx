import type { ChangeEvent, JSX } from 'react';

type Props = {
  teams: string[];
  chosen: string[];
  builds: Map<string, string[]>;
  current: (team: string) => string | undefined;
  onChange: (event: ChangeEvent<HTMLSelectElement>) => void;
};

export const BuildSelectors = ({
  teams,
  chosen,
  builds,
  current,
  onChange,
}: Props): JSX.Element => (
  <div className="duel-builds">
    {teams.map((team, index) => {
      const own = builds.get(team) ?? [];
      return (
        <label key={team}>
          <span>Version de {team}</span>
          <select
            value={chosen[index]}
            name={String(index)}
            onChange={onChange}
            disabled={own.length === 0}
          >
            {own.map((build, buildIndex) => (
              <option key={build} value={build}>
                {`v${buildIndex + 1} (${build})${build === current(team) ? ', actuelle' : ''}`}
              </option>
            ))}
            <option value="*">Toutes les versions</option>
          </select>
        </label>
      );
    })}
  </div>
);
