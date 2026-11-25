import {
  CALIBRATION,
  PREVIEW,
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
import { timeFormat } from '@/lib/format';
import type { TTournament, TTournamentIndex } from '@/types/tournament.types';
import { useCallback, useMemo, useState } from 'react';

type TTournamentOption = { value: string; label: string };

type TTournamentList = TTournamentIndex['tournaments'];

type TTournamentSelection = {
  options: TTournamentOption[];
  id: string;
};

function tournamentOptions(
  tournaments: TTournamentList,
  previewText: string | null | undefined,
  ready: boolean,
  shown: string | null,
): TTournamentSelection {
  const hasPreview = previewText !== undefined && previewText !== null && previewText !== '';
  const options = [
    ...(hasPreview ? [{ value: PREVIEW, label: 'Prochain (aperçu)' }] : []),
    ...tournaments.map((tournament) => ({
      value: tournament.id,
      label: shortDate.format(new Date(tournament.ts)),
    })),
  ];
  const ids = options.map((option) => option.value);
  const runningId = tournaments.find((tournament) => (tournament.winner ?? '') === '')?.id;
  const id = ready
    ? ([shown, runningId, hasPreview ? PREVIEW : undefined, ids[0]].find(
        (value) => value !== null && value !== undefined && value !== '' && ids.includes(value),
      ) ?? '')
    : '';
  return { options, id };
}

function readTournament(text: string | null | undefined): TTournament | null {
  if (text === null || text === undefined || text === '') {
    return null;
  }
  // oxlint-disable-next-line typescript/no-unsafe-return -- The ladder and configurable data sources must supply the TTournament format documented in docs/formats.md.
  return JSON.parse(text);
}

type TView = {
  index: TTournamentIndex | undefined;
  dataUpdatedAt: number;
  error: Error | null;
  ready: boolean;
  id: string;
  view: TTournament | null;
  preview: TTournament | null;
  text: string | null | undefined;
  factor: number;
  builds: Map<string, string[]>;
  options: TTournamentOption[];
  choose: (value: string) => void;
  pulses: number;
};

export function useTournamentView(): TView {
  const { data: index, dataUpdatedAt } = useGetTournamentIndex();
  const { data: summary } = useGetSummary();
  const { data: previewText, isFetched: previewFetched } = useGetTournamentPreview();
  const [choice, setChoice] = useSearchParam('tournament');
  // Tableau montré : gardé tant qu'il existe, même quand le défaut change.
  const [shown, setShown] = useState<string | null>(choice);
  const tournaments = useMemo(() => index?.tournaments ?? [], [index]);
  const recent = tournaments
    .filter((tournament) => Boolean(tournament.winner))
    .slice(0, CALIBRATION);
  const finished = useGetFinishedTournaments(recent.map((tournament) => tournament.id));
  const factor = correction(finished.flatMap((query) => (query.data ? [query.data] : [])));
  const builds = useMemo(() => new Map(Object.entries(summary?.builds ?? {})), [summary]);
  // Par défaut : le tournoi en cours, sinon l'aperçu du prochain, sinon le dernier.
  const ready = index !== undefined && previewFetched;
  const { options, id } = useMemo(
    () => tournamentOptions(tournaments, previewText, ready, shown),
    [tournaments, previewText, ready, shown],
  );
  if (ready && id !== (shown ?? '')) {
    setShown(id || null);
  }
  const { data: viewText, error } = useGetTournamentText(id && id !== PREVIEW ? id : null);
  let text = id === '' ? null : viewText;
  if (id === PREVIEW) {
    text = previewText;
  }
  const view = useMemo(() => readTournament(text), [text]);
  const preview = useMemo(() => readTournament(previewText), [previewText]);
  // Pulse quand le tableau affiché change, pas quand on en choisit un autre.
  const pulses = usePulse(text ?? undefined, id);
  const choose = useCallback(
    (value: string): void => {
      setChoice(value);
      setShown(value);
    },
    [setChoice],
  );
  return {
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
  };
}

export function tournamentDate(value: string | null | undefined): Date | null {
  if (value === null || value === undefined || value === '') {
    return null;
  }
  return new Date(value);
}

export function tournamentTiming(
  tournament: TTournament,
  factor: number,
  dataUpdatedAt: number,
): string {
  if ((tournament.winner ?? '') !== '') {
    const ended = tournamentDate(tournament.ended);
    if (ended === null) {
      return '';
    }
    const raw = tournamentDuration(tournament, true);
    return ` · a duré ${span(ended.getTime() - Date.parse(tournament.ts))}${raw === null ? '' : ` (prévu au tirage ≈ ${span(raw)})`}`;
  }
  const planned = predicted(tournament, factor);
  if (planned === null) {
    return '';
  }
  if (tournament.id === PREVIEW) {
    return ` · durée prévue ≈ ${span(planned)}`;
  }
  return ` · fin prévue vers ${timeFormat.format(new Date(dataUpdatedAt + (predicted(tournament, factor, false) ?? Number.NaN)))}`;
}

export type { TTournament } from '@/types/tournament.types';

export function useTournamentFeeds(): {
  model: ReturnType<typeof useGetModel>['data'];
  running: ReturnType<typeof useGetRunning>['data'];
} {
  const { data: model } = useGetModel();
  const { data: running } = useGetRunning();
  return { model, running };
}
