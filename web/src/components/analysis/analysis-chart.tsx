import type { FC, MouseEvent } from 'react';

const W = 600;
const H = 120;

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
  reference?: number;
  onPick: (k: number) => void;
};

// Courbe ou barres d'une mesure au fil de la partie ; un clic choisit la position. Le clavier et la
// liste des coups font la même chose : le graphe n'est qu'un raccourci.
const AnalysisChart: FC<Props> = ({
  title,
  lines = [],
  bars = [],
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
    ...lines.flatMap((l) => l.values),
    ...bars.map((b) => b.value),
  );
  const x = (k: number) => (k / Math.max(1, last)) * W;
  const y = (v: number) => H - (v / max) * (H - 4);
  const pick = (e: MouseEvent<SVGSVGElement>) => {
    const box = e.currentTarget.getBoundingClientRect();
    onPick(Math.round(((e.clientX - box.left) / box.width) * last));
  };
  const mark = (k: number, className: string) => (
    <line key={`${className}-${k}`} className={className} x1={x(k)} x2={x(k)} y1={0} y2={H} />
  );

  return (
    <figure className="analysis-chart">
      <figcaption>
        {title} <span>max {Math.round(max).toLocaleString('fr-CA')}</span>
      </figcaption>
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden="true" onClick={pick}>
        {bars.map((b) => (
          <rect
            key={b.at}
            x={x(b.at - 1) + 0.5}
            width={Math.max(1, x(1) - 1)}
            y={y(b.value)}
            height={H - y(b.value)}
            fill={b.color}
            opacity={b.at > cursor ? 0.3 : 1}
          />
        ))}
        {reference !== undefined && (
          <line className="chart-reference" x1={0} x2={W} y1={y(reference)} y2={y(reference)} />
        )}
        {crit.map((k) => mark(k, 'chart-crit'))}
        {tip !== null && mark(tip, 'chart-tip')}
        {mark(cursor, 'chart-cursor')}
        {lines.map((l) => (
          <polyline
            key={l.color}
            points={l.values.map((v, k) => `${x(k)},${y(v)}`).join(' ')}
            stroke={l.color}
          />
        ))}
      </svg>
    </figure>
  );
};

export { AnalysisChart };
