// Page Tournoi : tableau à double élimination d'un tournoi hebdomadaire, lu dans data/tournaments/, ou
// aperçu du prochain (data/tournament-preview.json). ?tournament= suit le tableau affiché.

import { LiveStatus } from '@/components/controls/live-status';
import { SessionSelect } from '@/components/controls/session-select';
import { Masthead } from '@/components/masthead/masthead';
import { Bracket } from '@/components/tournament/bracket';
import { Places } from '@/components/tournament/places';
import { PREVIEW, dateFormat, predicted, span } from '@/components/tournament/tournament.utils';
import { fullFormat, timeFormat } from '@/lib/format';
import { withProviders } from '@/providers/with-providers';
import type { TTournament } from './tournoi-data';
import { useCallback, useMemo } from 'react';
import type { ReactNode } from 'react';
import {
  tournamentDate,
  tournamentTiming,
  useTournamentFeeds,
  useTournamentView,
} from './tournoi-data';

type TTournamentHeading = {
  title: string;
  meta: string;
};

function tournamentHeading(
  view: TTournament | null,
  factor: number,
  dataUpdatedAt: number,
): TTournamentHeading {
  let title = 'Tournoi hebdomadaire';
  let meta = '';
  if (view) {
    const pace = `${view.time_ms < 1000 ? `${view.time_ms} ms` : `${view.time_ms / 1000} s`} par coup${tournamentTiming(view, factor, dataUpdatedAt)}`;
    if (view.id === PREVIEW) {
      title = 'Aperçu du prochain tournoi';
      meta = `${view.entrants.length} équipes · ${pace} · le tirage si le tournoi partait maintenant, selon l'Elo vérifié de la session, recalculé après chaque partie`;
    } else {
      title = `Tournoi du ${dateFormat.format(new Date(view.ts))}`;
      const winner = (view.winner ?? '') === '' ? '' : ` · vainqueur : ${view.winner}`;
      meta = `${view.entrants.length} équipes · ${pace} · tiré le ${fullFormat.format(new Date(view.ts))}${winner}`;
    }
  }

  return { title, meta };
}

function showEmpty(
  ready: boolean,
  view: TTournament | null,
  id: string,
  text: string | null | undefined,
  error: Error | null,
): boolean {
  return ready && view === null && (id === '' || Boolean(text) || error !== null);
}

const TournoiPageContent = (): ReactNode => {
  const {
    index,
    dataUpdatedAt,
    error,
    ready,
    id,
    view,
    preview,
    text,
    factor,
    builds,
    options,
    choose,
    pulses,
  } = useTournamentView();
  const { model, running } = useTournamentFeeds();
  const entrants = useMemo(
    () => new Map(view?.entrants.map((entrant) => [entrant.team, entrant])),
    [view],
  );
  const describe = useCallback(
    (team: string): string => {
      const entrant = entrants.get(team);
      const version = (builds.get(team)?.indexOf(entrant?.build ?? '') ?? -1) + 1;
      return [
        team,
        version ? `v${version}` : '',
        entrant ? `· ${Math.round(entrant.rating)} Elo` : '',
      ]
        .filter(Boolean)
        .join(' ');
    },
    [entrants, builds],
  );
  const next = tournamentDate(index?.next);
  const length = preview ? predicted(preview, factor) : null;
  const { title, meta } = tournamentHeading(view, factor, dataUpdatedAt);
  const empty = showEmpty(ready, view, id, text, error);
  let status = 'Chargement du tournoi…';
  let dot: 'error' | 'on' | '' = '';
  if (index) {
    status = `À jour à ${timeFormat.format(new Date(dataUpdatedAt))}`;
    dot = 'on';
  }
  if (error) {
    status = `Tournoi indisponible : ${error.message}`;
    dot = 'error';
  }
  const indicator = useMemo(
    () => <LiveStatus dot={dot} text={status} pulses={pulses} />,
    [dot, status, pulses],
  );

  return (
    <>
      <Masthead current="tournoi" status={indicator}>
        <SessionSelect
          id="tournament"
          label="Tournoi"
          value={id}
          options={options}
          disabled={options.length < 2}
          onChange={choose}
        />
      </Masthead>

      <main className="layout tournament-layout">
        <section className="section tournament" aria-labelledby="tournament-title">
          <div className="tournament-head">
            <div>
              <h2 id="tournament-title">{title}</h2>
              <p id="tournament-meta" className="tournament-meta">
                {meta}
              </p>
            </div>
            <p id="next" className="next" hidden={!next}>
              {next &&
                `Prochain tournoi : ${dateFormat.format(next)}, ${timeFormat.format(next)}${length !== null && length !== 0 ? `, durée prévue ≈ ${span(length)}` : ''}. Les jars acceptés avant le tirage jouent.`}
            </p>
          </div>
          {view ? (
            <Places view={view} />
          ) : (
            <ol id="places" className="places" aria-label="Classement final" hidden />
          )}
          <p id="tournament-empty" className="empty" hidden={!empty}>
            Aucun tournoi pour l&apos;instant. Chaque semaine, le ladder joue une répétition du
            tournoi final : double élimination à 5 s par coup, entre les équipes qui ont un jar
            accepté.
          </p>
          {view ? (
            <Bracket view={view} model={model} running={running} describe={describe} />
          ) : (
            <div id="bracket" className="bracket" />
          )}
        </section>
      </main>
    </>
  );
};

export const TournoiPage = withProviders(TournoiPageContent);
