import { describe } from '@/lib/analysis';
import type { TAnalysis } from '@/lib/analysis';
import type { TColor } from '@/types/games.types';

const COLORS = ['red', 'black'] as const;
const STAT_KEYS = ['min', 'mean', 'median', 'max', 'sd', 'n'] as const;
const HEADERS = ['Min', 'Moyenne', 'Médiane', 'Max', 'Écart type', 'N'];

type Point = { color: TColor; value: number };

type Props = {
  analysis: TAnalysis;
  timed: { color: TColor; ms: number }[];
  depths: Point[];
  rates: Point[];
  name: (color: TColor) => string;
};

type Measure = { label: string; values: Record<TColor, number[]> };

function pointsByColor(points: Point[]): Record<TColor, number[]> {
  const values: Record<TColor, number[]> = { red: [], black: [] };
  for (const point of points) {
    values[point.color].push(point.value);
  }
  return values;
}

function statRows(measure: Measure, name: Props['name']): React.JSX.Element[] {
  return COLORS.map((color) => {
    const stats = describe(measure.values[color]);
    return (
      <tr key={`${measure.label}-${color}`}>
        {color === 'red' && (
          <th scope="rowgroup" rowSpan={2}>
            {measure.label}
          </th>
        )}
        <td>{name(color)}</td>
        {stats ? (
          STAT_KEYS.map((key) => (
            <td key={key} className="tabular-nums">
              {stats[key].toLocaleString('fr-CA', { maximumFractionDigits: 1 })}
            </td>
          ))
        ) : (
          <td colSpan={6}>–</td>
        )}
      </tr>
    );
  });
}

export function AnalysisStats({ analysis, timed, depths, rates, name }: Props): React.JSX.Element {
  const measures: Measure[] = [
    { label: 'Territoire', values: { red: analysis.red.terr, black: analysis.black.terr } },
    { label: 'Mobilité', values: { red: analysis.red.mob, black: analysis.black.mob } },
    { label: 'Région', values: { red: analysis.red.reg, black: analysis.black.reg } },
  ];
  if (timed.length > 0) {
    measures.unshift({
      label: 'Temps par coup (ms)',
      values: pointsByColor(timed.map(({ color, ms }) => ({ color, value: ms }))),
    });
  }
  if (depths.length > 0) {
    measures.push({ label: 'Profondeur du bot', values: pointsByColor(depths) });
  }
  if (rates.length > 0) {
    measures.push({ label: 'Nœuds par seconde', values: pointsByColor(rates) });
  }
  return (
    <section className="section analyse-stats" aria-labelledby="stats-title">
      <h2 id="stats-title">Statistiques</h2>
      <div className="analyse-scroll">
        <table aria-labelledby="stats-title" className="analyse-table">
          <thead>
            <tr>
              <th scope="col">Mesure</th>
              <th scope="col">Couleur</th>
              {HEADERS.map((header) => (
                <th key={header} scope="col">
                  {header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>{measures.map((measure) => statRows(measure, name))}</tbody>
        </table>
      </div>
    </section>
  );
}
