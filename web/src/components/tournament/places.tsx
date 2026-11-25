import { cn } from '@/lib/utils';
import type { TTournament } from '@/types/tournament.types';
import type { FC } from 'react';

type Props = {
  view: TTournament;
};

// Classement final, une fois le tournoi joué.
const Places: FC<Props> = ({ view }) => {
  const ranked = view.places
    ? view.entrants.map((e, i) => ({ team: e.team, place: view.places![i] }))
    : [];
  ranked.sort((a, b) => a.place - b.place || a.team.localeCompare(b.team));

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
