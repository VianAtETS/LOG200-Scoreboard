import type { TPaceStat } from '@/types/summary.types';
import { useCallback } from 'react';
import type { ChangeEvent, JSX, MouseEvent } from 'react';
import { ELO_STATS, PACE_STATS, RECORD_STATS, SORT_COLUMNS, statOf } from './ranking.data';
import type { TEloStat, TRankingSort, TRecordStat, TSortColumn } from './ranking.data';

type TMeasures = {
  eloStat: TEloStat;
  recordStat: TRecordStat;
  paceStat: TPaceStat;
};

type TSettings = TMeasures & {
  onEloChange: (stat: TEloStat) => void;
  onRecordChange: (stat: TRecordStat) => void;
  onPaceChange: (stat: TPaceStat) => void;
};

export function RankingTools({
  eloStat,
  recordStat,
  paceStat,
  onEloChange,
  onRecordChange,
  onPaceChange,
}: TSettings): JSX.Element {
  const change = useCallback(
    (event: ChangeEvent<HTMLSelectElement>): void => {
      const { id, value } = event.currentTarget;
      if (id === 'elo-stat') {
        onEloChange(statOf(ELO_STATS, value, eloStat));
      } else if (id === 'record-stat') {
        onRecordChange(statOf(RECORD_STATS, value, recordStat));
      } else {
        onPaceChange(statOf(PACE_STATS, value, paceStat));
      }
    },
    [eloStat, recordStat, paceStat, onEloChange, onRecordChange, onPaceChange],
  );
  const settings = [
    {
      id: 'elo-stat',
      label: 'Score',
      value: eloStat,
      title: ELO_STATS[eloStat][1],
      options: Object.entries(ELO_STATS),
    },
    {
      id: 'record-stat',
      label: 'Bilan',
      accessibleName: 'Bilan et parties',
      value: recordStat,
      title: RECORD_STATS[recordStat][1],
      options: [
        ['all', ['Total', RECORD_STATS.all[1]]],
        ['build', ['Actuel', RECORD_STATS.build[1]]],
      ] as const,
    },
    {
      id: 'pace-stat',
      label: 'Temps',
      accessibleName: 'Temps par coup',
      value: paceStat,
      title: PACE_STATS[paceStat][1],
      options: Object.entries(PACE_STATS),
    },
  ];
  return (
    // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- This labelled group is part of the flex toolbar, outside a form.
    <div className="ranking-tools" role="group" aria-label="Mesures affichées">
      {settings.map(({ id, label, accessibleName, value, title, options }) => (
        <label key={id} htmlFor={id}>
          <span>{label}</span>
          <select id={id} aria-label={accessibleName} value={value} title={title} onChange={change}>
            {options.map(([key, [name, tooltip]]) => (
              <option key={key} value={key} title={tooltip}>
                {name}
              </option>
            ))}
          </select>
        </label>
      ))}
    </div>
  );
}

type TColumn = { key: TSortColumn; label: string; title?: string; cell: string };

type THeaderProps = TMeasures & {
  sort: TRankingSort;
  onSortChange: (sort: TRankingSort) => void;
};

function rankingColumns({ eloStat, recordStat, paceStat }: TMeasures): TColumn[] {
  const [eloLabel, eloTitle] = ELO_STATS[eloStat];
  const [recordLabel, recordTitle] = RECORD_STATS[recordStat];
  const [paceLabel, paceTitle] = PACE_STATS[paceStat];
  return [
    { key: 'rank', label: '#', title: 'Rang officiel selon l’Elo actuel', cell: 'num' },
    { key: 'team', label: SORT_COLUMNS.team, cell: 'team' },
    { key: 'elo', label: eloLabel, title: eloTitle, cell: 'num' },
    {
      key: 'record',
      label: recordLabel,
      title: `${recordTitle}. Tri par victoires, puis moins de défaites.`,
      cell: 'num record',
    },
    {
      key: 'games',
      label: SORT_COLUMNS.games,
      title:
        recordStat === 'build'
          ? 'Nombre de parties, version actuelle'
          : 'Nombre de parties, toutes versions',
      cell: 'num',
    },
    {
      key: 'pct',
      label: SORT_COLUMNS.pct,
      title: 'Pourcentage de victoires, version actuelle',
      cell: 'num pct',
    },
    { key: 'pace', label: paceLabel, title: `${paceTitle}, version actuelle`, cell: 'num pace' },
    { key: 'last', label: 'Dernière', title: 'Date de la dernière partie', cell: 'num last-col' },
  ];
}

// oxlint-disable-next-line react/no-multi-comp -- These controls share the ranking measures and their labels.
export function RankingHeaders({
  eloStat,
  recordStat,
  paceStat,
  sort,
  onSortChange,
}: THeaderProps): JSX.Element {
  const handleSort = useCallback(
    (event: MouseEvent<HTMLButtonElement>): void => {
      const column = statOf(SORT_COLUMNS, event.currentTarget.value, sort.column);
      onSortChange({
        column,
        direction:
          sort.column === column && sort.direction === 'ascending' ? 'descending' : 'ascending',
      });
    },
    [sort, onSortChange],
  );
  return (
    <thead>
      <tr>
        {rankingColumns({ eloStat, recordStat, paceStat }).map(({ key, label, title, cell }) => (
          <th
            key={key}
            scope="col"
            className={cell}
            aria-sort={sort.column === key ? sort.direction : undefined}
          >
            <button
              type="button"
              className="th-sort"
              title={title}
              value={key}
              onClick={handleSort}
            >
              {key === 'rank' && <span className="sr-only">Rang officiel </span>}
              {label}
              <span className="sort-direction" aria-hidden="true" />
            </button>
          </th>
        ))}
      </tr>
    </thead>
  );
}
