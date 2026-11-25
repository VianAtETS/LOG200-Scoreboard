// Page Direct : toutes les parties en cours, coup par coup, depuis /api/live. Un clic ouvre la partie
// dans le replay de la page Classement, qui la suit en direct.

import { LiveStatus } from '@/components/controls/live-status';
import { LiveCard } from '@/components/live/live-card';
import { Masthead } from '@/components/masthead/masthead';
import { useGetModel } from '@/dao/hooks/use-ladder';
import { useGetTournaments } from '@/dao/hooks/use-tournaments';
import { useInterval } from '@/hooks/use-interval';
import { useLiveGames } from '@/hooks/use-live-games';
import { forget } from '@/lib/live';
import { withProviders } from '@/providers/with-providers';
import { useState } from 'react';
import type { JSX } from 'react';

const EXPIRE_MS = 250;

const STATES = {
  connecting: 'Connexion au direct…',
  unavailable: 'Direct indisponible sur ce serveur.',
  reconnecting: 'Connexion perdue, reconnexion…',
  outdated: 'Page à mettre à jour : rechargez-la.',
} as const;

const DirectPageContent = (): JSX.Element => {
  const live = useLiveGames();
  const { data: model } = useGetModel();
  const [now, setNow] = useState(() => performance.now());

  // Une partie terminée reste affichée ENDED_KEEP_MS, puis disparaît.
  const games = forget(live.games, now);
  const shown = [...games.values()];
  useInterval(() => setNow(performance.now()), shown.some((game) => game.ended) ? EXPIRE_MS : null);

  const ids = [...new Set(shown.flatMap((game) => (game.tournament ? [game.tournament.id] : [])))];
  const brackets = useGetTournaments(ids);
  const bracketOf = (id: string): (typeof brackets)[number]['data'] =>
    brackets[ids.indexOf(id)]?.data;

  const on = live.state === 'on';
  let dot: 'on' | '' | 'error' = 'error';
  if (on) {
    dot = 'on';
  } else if (live.state === 'connecting') {
    dot = '';
  }
  // oxlint-disable-next-line react-perf/jsx-no-jsx-as-prop -- Masthead places this changing status outside its navigation.
  const status = (
    <LiveStatus
      dot={dot}
      text={live.state === 'on' ? `${shown.length} parties en cours` : STATES[live.state]}
    />
  );

  return (
    <>
      <Masthead current="direct" status={status} />

      <main className="layout tournament-layout">
        <section className="section tournament" aria-labelledby="direct-title">
          <h2 id="direct-title">Parties en cours</h2>
          <p id="direct-empty" className="empty" hidden={shown.length > 0 || !on}>
            Aucune partie en cours. Les parties du ladder et du tournoi apparaissent ici dès leur
            premier coup.
          </p>
          <div id="grid" className="live-grid">
            {shown.map((game) => {
              const bracket = game.tournament ? bracketOf(game.tournament.id) : undefined;
              return bracket === undefined ? (
                <LiveCard key={game.id} game={game} model={model} />
              ) : (
                <LiveCard key={game.id} game={game} model={model} bracket={bracket} />
              );
            })}
          </div>
        </section>
      </main>
    </>
  );
};

export const DirectPage = withProviders(DirectPageContent);
