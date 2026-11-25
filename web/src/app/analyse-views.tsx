import { Board } from '@/components/board/board';
import { FirstIcon, LastIcon, NextIcon, PrevIcon } from '@/components/icons/icons';
import type { TAnalysis, TSeries } from '@/lib/analysis';
import { signed } from '@/lib/format';
import { squareName } from '@/lib/replay';
import type { TStep } from '@/lib/replay';
import { cn } from '@/lib/utils';
import type { TColor, TGame } from '@/types/games.types';
import type { MouseEvent } from 'react';
import { AnalysisChart } from '@/components/analysis/analysis-chart';

const COLORS = ['red', 'black'] as const;

// Le pion noir se fond dans le fond sombre : le texte reste lisible dans les deux thèmes.
const LEGEND = { red: { color: 'var(--color-pawn-red)' }, black: { color: 'var(--color-text)' } };

const MEASURES: { key: keyof TSeries; label: string; hint: string }[] = [
  { key: 'terr', label: 'Territoire', hint: "cases atteintes avant l'adversaire" },
  { key: 'mob', label: 'Mobilité', hint: 'cases atteignables en un coup' },
  { key: 'reg', label: 'Région', hint: 'cases encore accessibles' },
];

type Navigation = { k: number; last: number; go: (index: number) => void };

type Names = { name: (color: TColor) => string };

export type Point = { color: TColor; at: number; value: number };

export type Timed = { color: TColor; at: number; ms: number };

export function renderBoard({
  step,
  k,
  last,
  prevCrit,
  nextCrit,
  go,
  analysis,
  name,
}: Navigation &
  Names & {
    step: TStep;
    prevCrit: number | undefined;
    nextCrit: number | undefined;
    analysis: TAnalysis;
  }): React.JSX.Element {
  const controls = [
    { label: 'Coup critique précédent', index: prevCrit, Icon: FirstIcon },
    { label: 'Coup précédent', index: k > 0 ? k - 1 : undefined, Icon: PrevIcon },
    { label: 'Coup suivant', index: k < last ? k + 1 : undefined, Icon: NextIcon },
    { label: 'Coup critique suivant', index: nextCrit, Icon: LastIcon },
  ];
  // oxlint-disable-next-line react-perf/jsx-no-new-function-as-prop -- Native buttons do not skip renders based on handler identity.
  const select = (event: MouseEvent<HTMLButtonElement>): void =>
    go(Number(event.currentTarget.value));
  return (
    <section className="section analyse-board" aria-label="Position analysée">
      <div className="board-wrap">
        <Board step={step} index={k} label={`Position après le coup ${k} sur ${last}`} />
      </div>
      <div className="replay-controls">
        {controls.map(({ label, index, Icon }) => (
          <button
            key={label}
            className="icon"
            type="button"
            aria-label={label}
            disabled={index === undefined}
            value={index}
            onClick={select}
          >
            <Icon />
          </button>
        ))}
      </div>
      <p className="analyse-keys">← → coup par coup, ↑ ↓ coups critiques, Début et Fin.</p>
      <table className="analyse-measures">
        <caption>
          {k
            ? `Après le coup ${k} sur ${last}${step.opening ? ' (ouverture)' : ''}`
            : 'Position initiale'}
        </caption>
        <thead>
          <tr>
            <th scope="col">Mesure</th>
            {COLORS.map((color) => (
              <th key={color} scope="col">
                {name(color)}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {MEASURES.map((measure) => (
            <tr key={measure.key}>
              <th scope="row" title={measure.hint}>
                {measure.label}
              </th>
              {COLORS.map((color) => {
                const values = analysis[color][measure.key];
                const value = values[k];
                const previous = values[k - 1];
                const delta = value === undefined || previous === undefined ? 0 : value - previous;
                return (
                  <td key={color}>
                    {value}
                    {delta !== 0 && <em className={delta > 0 ? 'up' : 'down'}> {signed(delta)}</em>}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

export function renderMoves({
  steps,
  k,
  go,
  analysis,
  moverOf,
}: Pick<Navigation, 'k' | 'go'> & {
  steps: TStep[];
  analysis: TAnalysis;
  moverOf: (index: number) => TColor | undefined;
}): React.JSX.Element {
  // oxlint-disable-next-line react-perf/jsx-no-new-function-as-prop -- Native buttons do not skip renders based on handler identity.
  const select = (event: MouseEvent<HTMLButtonElement>): void =>
    go(Number(event.currentTarget.value));
  return (
    <section className="section analyse-moves" aria-labelledby="moves-title">
      <h2 id="moves-title">Coups</h2>
      <p className="analyse-note">
        <span className="crit-fine">▲</span> a gagné du territoire,{' '}
        <span className="crit-bad">▼</span> en a coûté, <span className="crit-tip">⚑</span> point de
        bascule : après lui, le perdant ne reprend jamais l&apos;avantage.
      </p>
      <ol className="move-list">
        {steps.map((step, index) =>
          step.move ? (
            <li key={step.move.destroyed}>
              <button
                type="button"
                className={cn(index === k && 'current', step.opening && 'opening')}
                aria-current={index === k}
                value={index}
                onClick={select}
              >
                <span className="move-number">{index}</span>
                <i
                  className={cn('pawn', moverOf(index))}
                  aria-label={moverOf(index) === 'red' ? 'rouge' : 'noir'}
                />
                {`${squareName(step.move.from)} → ${squareName(step.move.to)} ✕ ${squareName(step.move.destroyed)}`}
                {analysis.crit.get(index) === 'fine' && (
                  <span className="crit-fine" title="A gagné du territoire">
                    {' '}
                    ▲
                  </span>
                )}
                {analysis.crit.get(index) === 'bad' && (
                  <span className="crit-bad" title="A coûté du territoire">
                    {' '}
                    ▼
                  </span>
                )}
                {analysis.tip === index && (
                  <span className="crit-tip" title="Point de bascule">
                    {' '}
                    ⚑
                  </span>
                )}
              </button>
            </li>
          ) : null,
        )}
      </ol>
    </section>
  );
}

type ChartData = {
  title: string;
  lines: { color: string; values: number[] }[];
  bars: { color: string; at: number; value: number }[];
  reference?: number | undefined;
};

export function renderCharts({
  analysis,
  game,
  timed,
  depths,
  rates,
  last,
  k,
  crit,
  go,
  name,
}: Navigation &
  Names & {
    analysis: TAnalysis;
    game: TGame;
    timed: Timed[];
    depths: Point[];
    rates: Point[];
    crit: number[];
  }): React.JSX.Element {
  const charts: ChartData[] = MEASURES.map((measure) => ({
    title: `${measure.label} : ${measure.hint}`,
    lines: COLORS.map((color) => ({
      color: LEGEND[color].color,
      values: analysis[color][measure.key],
    })),
    bars: [],
  }));
  if (timed.length > 0) {
    charts.push({
      title: `Temps par coup (ms)${game.time_ms !== undefined && game.time_ms !== 0 ? `, ${game.time_ms} ms alloués` : ''}`,
      lines: [],
      bars: timed.map((time) => ({ color: LEGEND[time.color].color, value: time.ms, at: time.at })),
      reference: game.time_ms,
    });
  }
  for (const { title, points } of [
    { title: 'Profondeur terminée par le bot', points: depths },
    { title: 'Nœuds par seconde du bot', points: rates },
  ]) {
    if (points.length > 0) {
      charts.push({
        title,
        lines: [],
        bars: points.map(({ color, at, value }) => ({ color: LEGEND[color].color, at, value })),
      });
    }
  }
  return (
    <section className="section analyse-charts" aria-labelledby="charts-title">
      <h2 id="charts-title">Au fil de la partie</h2>
      <p className="analyse-legend">
        {COLORS.map((color) => (
          <span key={color} className="legend-line" style={LEGEND[color]}>
            <span>{name(color)}</span>
          </span>
        ))}
        <span className="legend-mark chart-cursor">position affichée</span>
        {crit.length > 0 && <span className="legend-mark chart-crit">coups critiques</span>}
        {analysis.tip !== null && <span className="legend-mark chart-tip">bascule</span>}
      </p>
      {charts.map((chart) => (
        <AnalysisChart
          key={chart.title}
          title={chart.title}
          lines={chart.lines}
          bars={chart.bars}
          reference={chart.reference}
          last={last}
          cursor={k}
          crit={crit}
          tip={analysis.tip}
          onPick={go}
        />
      ))}
    </section>
  );
}
