import { useCallback } from 'react';
import type { FC, MouseEvent, ReactElement } from 'react';

const WIDTH = 600;
const HEIGHT = 120;
const TOP_PADDING = 4;
const HALF_PIXEL = 0.5;
const FUTURE_OPACITY = 0.3;
const EMPTY_LINES: NonNullable<Props['lines']> = [];
const EMPTY_BARS: NonNullable<Props['bars']> = [];

type Props = {
  title: string;
  // Une valeur par position (0 = plateau initial) : courbes.
  lines?: { color: string; values: number[] }[];
  // Une barre par coup, à la position `at` qu'il produit.
  bars?: { color: string; value: number; at: number }[];
  // Dernière position.
  last: number;
  cursor: number;
  crit: number[];
  tip: number | null;
  // Ligne horizontale pointillée : le temps alloué par coup.
  reference?: number | undefined;
  onPick: (k: number) => void;
};

// Courbe ou barres d'une mesure au fil de la partie ; un clic choisit la position. Le clavier et la
// liste des coups font la même chose : le graphe n'est qu'un raccourci.
const AnalysisChart: FC<Props> = ({
  title,
  lines = EMPTY_LINES,
  bars = EMPTY_BARS,
  last,
  cursor,
  crit,
  tip,
  reference,
  onPick,
}) => {
  const max = Math.max(
    1,
    reference ?? 0,
    ...lines.flatMap((line) => line.values),
    ...bars.map((bar) => bar.value),
  );
  const x = (position: number): number => (position / Math.max(1, last)) * WIDTH;
  const y = (value: number): number => HEIGHT - (value / max) * (HEIGHT - TOP_PADDING);
  const pick = useCallback(
    (event: MouseEvent<SVGSVGElement>): void => {
      const box = event.currentTarget.getBoundingClientRect();
      onPick(Math.round(((event.clientX - box.left) / box.width) * last));
    },
    [last, onPick],
  );
  const mark = (position: number, className: string): ReactElement => (
    <line
      key={`${className}-${position}`}
      className={className}
      x1={x(position)}
      x2={x(position)}
      y1={0}
      y2={HEIGHT}
    />
  );

  return (
    <figure className="analysis-chart">
      <figcaption>
        {title} <span>max {Math.round(max).toLocaleString('fr-CA')}</span>
      </figcaption>
      <svg
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        preserveAspectRatio="none"
        aria-hidden="true"
        onClick={pick}
      >
        {bars.map((bar) => (
          <rect
            key={bar.at}
            x={x(bar.at - 1) + HALF_PIXEL}
            width={Math.max(1, x(1) - 1)}
            y={y(bar.value)}
            height={HEIGHT - y(bar.value)}
            fill={bar.color}
            opacity={bar.at > cursor ? FUTURE_OPACITY : 1}
          />
        ))}
        {reference !== undefined && (
          <line className="chart-reference" x1={0} x2={WIDTH} y1={y(reference)} y2={y(reference)} />
        )}
        {crit.map((position) => mark(position, 'chart-crit'))}
        {tip !== null && mark(tip, 'chart-tip')}
        {mark(cursor, 'chart-cursor')}
        {lines.map((line) => (
          <polyline
            key={line.color}
            points={line.values.map((value, position) => `${x(position)},${y(value)}`).join(' ')}
            stroke={line.color}
          />
        ))}
      </svg>
    </figure>
  );
};

export { AnalysisChart };
