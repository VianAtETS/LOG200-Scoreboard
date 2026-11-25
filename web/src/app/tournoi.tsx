// Page Tournoi : tableau à double élimination d'un tournoi hebdomadaire, lu dans data/tournaments/, ou
// aperçu du prochain (data/tournament-preview.json). ?tournament= suit le tableau affiché.

import { LiveStatus } from '@/components/controls/live-status';
import { SessionSelect } from '@/components/controls/session-select';
import { Masthead } from '@/components/masthead/masthead';
import { Bracket } from '@/components/tournament/bracket';
import { Places } from '@/components/tournament/places';
import {
  CALIBRATION,
  PREVIEW,
  dateFormat,
  predicted,
  shortDate,
  span,
} from '@/components/tournament/tournament.utils';
import { useGetModel, useGetRunning, useGetSummary } from '@/dao/hooks/use-ladder';
import {
  useGetFinishedTournaments,
  useGetTournamentIndex,
  useGetTournamentPreview,
  useGetTournamentText,
} from '@/dao/hooks/use-tournaments';
import { usePulse } from '@/hooks/use-pulse';
import { useSearchParam } from '@/hooks/use-search-param';
import { correction, tournamentDuration } from '@/lib/estimate';
import { fullFormat, timeFormat } from '@/lib/format';
import { withProviders } from '@/providers/with-providers';
import type { TTournament } from '@/types/tournament.types';
import { useMemo, useState } from 'react';

const TournoiPage = () => {
  const { data: index, dataUpdatedAt } = useGetTournamentIndex();
  const { data: summary } = useGetSummary();
  const { data: model } = useGetModel();
  const { data: running = [] } = useGetRunning();
  const { data: previewText, isFetched: previewFetched } = useGetTournamentPreview();
  const [choice, setChoice] = useSearchParam('tournament');
  // Tableau montré : gardé tant qu'il existe, même quand le défaut change.
  const [shown, setShown] = useState<string | null>(choice);

  const tournaments = useMemo(() => index?.tournaments ?? [], [index]);
  const recent = tournaments.filter((t) => t.winner).slice(0, CALIBRATION);
  const finished = useGetFinishedTournaments(recent.map((t) => t.id));
  const factor = correction(finished.flatMap((q) => (q.data ? [q.data] : [])));
  const builds = useMemo(() => new Map(Object.entries(summary?.builds ?? {})), [summary]);

  // Par défaut : le tournoi en cours, sinon l'aperçu du prochain, sinon le dernier.
  const options = [
    ...(previewText ? [{ value: PREVIEW, label: 'Prochain (aperçu)' }] : []),
    ...tournaments.map((t) => ({ value: t.id, label: shortDate.format(new Date(t.ts)) })),
  ];
  const ids = options.map((o) => o.value);
  const runningId = tournaments.find((t) => !t.winner)?.id;
  const ready = Boolean(index) && previewFetched;
  const id = ready
    ? ([shown, runningId, previewText && PREVIEW, ids[0]].find(
        (v): v is string => Boolean(v) && ids.includes(v!),
      ) ?? '')
    : '';

  if (ready && id !== (shown ?? '')) {
    setShown(id || null);
  }

  const { data: viewText, error } = useGetTournamentText(id && id !== PREVIEW ? id : null);
  const text = id === PREVIEW ? previewText : id ? viewText : null;
  const view = useMemo<TTournament | null>(() => (text ? JSON.parse(text) : null), [text]);
  const preview = useMemo<TTournament | null>(
    () => (previewText ? JSON.parse(previewText) : null),
    [previewText],
  );

  // Pulse quand le tableau affiché change, pas quand on en choisit un autre.
  const pulses = usePulse(text ?? undefined, id);

  const describe = (team: string) => {
    const entrant = view?.entrants.find((e) => e.team === team);
    const n = (builds.get(team)?.indexOf(entrant?.build ?? '') ?? -1) + 1;
    return [team, n ? `v${n}` : '', entrant ? `· ${Math.round(entrant.rating)} Elo` : '']
      .filter(Boolean)
      .join(' ');
  };

  const timing = (v: TTournament) => {
    if (v.winner) {
      if (!v.ended) {
        return '';
      }
      const raw = tournamentDuration(v, true);
      return ` · a duré ${span(Date.parse(v.ended) - Date.parse(v.ts))}${raw === null ? '' : ` (prévu au tirage ≈ ${span(raw)})`}`;
    }
    const planned = predicted(v, factor);
    if (planned === null) {
      return '';
    }
    if (v.id === PREVIEW) {
      return ` · durée prévue ≈ ${span(planned)}`;
    }
    return ` · fin prévue vers ${timeFormat.format(new Date(dataUpdatedAt + predicted(v, factor, false)!))}`;
  };

  const next = index?.next ? new Date(index.next) : null;
  const length = preview ? predicted(preview, factor) : null;

  let title = 'Tournoi hebdomadaire';
  let meta = '';
  if (view) {
    const pace = `${view.time_ms < 1000 ? `${view.time_ms} ms` : `${view.time_ms / 1000} s`} par coup${timing(view)}`;
    if (view.id === PREVIEW) {
      title = 'Aperçu du prochain tournoi';
      meta = `${view.entrants.length} équipes · ${pace} · le tirage si le tournoi partait maintenant, selon l'Elo vérifié de la session, recalculé après chaque partie`;
    } else {
      title = `Tournoi du ${dateFormat.format(new Date(view.ts))}`;
      const winner = view.winner ? ` · vainqueur : ${view.winner}` : '';
      meta = `${view.entrants.length} équipes · ${pace} · tiré le ${fullFormat.format(new Date(view.ts))}${winner}`;
    }
  }

  const status = error
    ? `Tournoi indisponible : ${error.message}`
    : index
      ? `À jour à ${timeFormat.format(new Date(dataUpdatedAt))}`
      : 'Chargement du tournoi…';

  return (
    <>
      <Masthead
        current="tournoi"
        status={
          <LiveStatus dot={error ? 'error' : index ? 'on' : ''} text={status} pulses={pulses} />
        }
      >
        <SessionSelect
          id="tournament"
          label="Tournoi"
          value={id}
          options={options}
          disabled={ids.length < 2}
          onChange={(value) => {
            setChoice(value);
            setShown(value);
          }}
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
                `Prochain tournoi : ${dateFormat.format(next)}, ${timeFormat.format(next)}${length ? `, durée prévue ≈ ${span(length)}` : ''}. Les jars acceptés avant le tirage jouent.`}
            </p>
          </div>
          {view ? (
            <Places view={view} />
          ) : (
            <ol id="places" className="places" aria-label="Classement final" hidden />
          )}
          <p
            id="tournament-empty"
            className="empty"
            hidden={!ready || Boolean(view) || (Boolean(id) && !text && !error)}
          >
            Aucun tournoi pour l'instant. Chaque semaine, le ladder joue une répétition du tournoi
            final : double élimination à 5 s par coup, entre les équipes qui ont un jar accepté.
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

export default withProviders(TournoiPage);
