import { pageUrl } from '@/lib/url';
import type { TDuelPair } from '@/types/games.types';
import type { JSX } from 'react';

export const PairTable = ({ pairs }: { pairs: TDuelPair[] }): JSX.Element => (
  <section className="section duel-pairs" aria-labelledby="pairs-title">
    <h2 id="pairs-title">Paires, la plus récente d&apos;abord</h2>
    <div className="analyse-scroll">
      <table aria-labelledby="pairs-title" className="analyse-table">
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
              {pair.games.map((id, gameIndex) => (
                <td key={id}>
                  <a href={pageUrl('analyse.html', { game: id })}>
                    {pair.winners[gameIndex]} gagne
                  </a>
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  </section>
);
