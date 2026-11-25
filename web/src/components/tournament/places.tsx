import { cn } from '@/lib/utils';
import type { TTournament } from '@/types/tournament.types';
import type { FC } from 'react';

type Props = {
  view: TTournament;
};

// Classement final, une fois le tournoi joué.
const Places: FC<Props> = ({ view }) => {
  const { places } = view;
  const ranked = places
    ? view.entrants.map((entrant, index) => ({ team: entrant.team, place: places[index] }))
    : [];
  ranked.sort(
    (left, right) =>
      Number(left.place) - Number(right.place) || left.team.localeCompare(right.team),
  );

  return (
    <ol id="places" className="places" aria-label="Classement final" hidden={ranked.length === 0}>
      {ranked.map(({ team, place }) => (
        <li key={team} className={cn(place === 1 && 'first')}>
          <span className="place">{place === 1 ? '1er' : `${place}e`}</span>
          <span>{team}</span>
        </li>
      ))}
    </ol>
  );
};

export { Places };
