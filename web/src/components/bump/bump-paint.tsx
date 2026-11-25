import { Fragment, useMemo } from 'react';
import type { FC } from 'react';
import type { TRankPoint } from '@/lib/bump';
import { cn } from '@/lib/utils';
import type { TRatingRow } from '@/types/games.types';
import { dateTicks, valueAt } from './bump.utils';
import type { TGeometry } from './bump.utils';

const muted = 'var(--color-text-muted)';
const MAX_LABEL_CHARS = 11;
const LABEL_SLICE_LEN = 10;
const AXIS_LABEL_OFFSET = 8;
const VERSION_LABEL_OFFSET = 9;

const MUTED_FILL = { fill: muted };
const MUTED_STROKE = { stroke: muted };

type Props = {
  geom: TGeometry;
  series: Map<string, TRankPoint[]>;
  columns: TRatingRow[];
  lines: { team: string; d: string; cached: boolean }[];
  palette: Map<string, string>;
  team: string | null;
  versionOf: (team: string, build?: string) => string;
};

const BumpGrid: FC<Pick<Props, 'geom' | 'columns'>> = ({ geom, columns }) => (
  <g className="bump-grid">
    {geom.ticks.map((value) => (
      <Fragment key={value}>
        <line
          x1={geom.margin.left}
          x2={geom.margin.left + geom.inner.w}
          y1={geom.y(value)}
          y2={geom.y(value)}
        />
        <text x={geom.margin.left - AXIS_LABEL_OFFSET} y={geom.y(value)} className="rank">
          {value}
        </text>
      </Fragment>
    ))}
    {dateTicks(columns, geom.inner.w).map((tick) => (
      <text
        key={tick.col}
        x={geom.x(tick.col)}
        y={geom.height - AXIS_LABEL_OFFSET}
        className="date"
        textAnchor={tick.anchor}
      >
        {tick.text}
      </text>
    ))}
  </g>
);

// oxlint-disable-next-line react/no-multi-comp -- The grid is a private layer of this SVG and shares its geometry.
const BumpPaint: FC<Props> = ({ geom, columns, lines, palette, team, versionOf, series }) => {
  const paint = useMemo(
    () =>
      new Map(
        [...palette].map(([id, color]) => [
          id,
          { fill: { fill: color }, stroke: { stroke: color } },
        ]),
      ),
    [palette],
  );
  const on = (candidate: string): boolean => candidate === team;
  return (
    <>
      <BumpGrid geom={geom} columns={columns} />
      {/* Changing stroke width rerasterizes dense paths during hover. */}
      <g className="bump-lines">
        {lines.map(({ team: itemTeam, d, cached }) => (
          <g
            key={itemTeam}
            className={cn(
              series.get(itemTeam)?.[0]?.bot === true && 'bot',
              on(itemTeam) && 'on',
              cached && 'cached',
            )}
            style={paint.get(itemTeam)?.stroke ?? MUTED_STROKE}
            data-team={itemTeam}
          >
            <path d={d} className="normal-stroke" />
            <path d={d} className="focus-stroke" />
          </g>
        ))}
      </g>
      <g className="bump-nodes">
        {geom.paintOrder.map((itemTeam) => {
          const points = series.get(itemTeam) ?? [];
          const path = geom.paths.get(itemTeam) ?? [];
          const [first] = path;
          const [firstPoint] = points;
          const fill = paint.get(itemTeam)?.fill ?? MUTED_FILL;
          return (
            <Fragment key={itemTeam}>
              {first && firstPoint && firstPoint.x > 0 && (
                <circle
                  cx={geom.x(first[0])}
                  cy={geom.y(first[1])}
                  r={3}
                  className={cn(on(itemTeam) && 'on')}
                  style={fill}
                  data-team={itemTeam}
                />
              )}
              {points.flatMap((pt) => {
                if (!pt.version) {
                  return [];
                }
                const cy = geom.y(valueAt(path, pt.x));
                return [
                  <Fragment key={pt.x}>
                    <circle
                      cx={geom.x(pt.x)}
                      cy={cy}
                      r={5}
                      className={cn('version', on(itemTeam) && 'on')}
                      style={fill}
                      data-team={itemTeam}
                    />
                    <text
                      x={geom.x(pt.x)}
                      y={cy - VERSION_LABEL_OFFSET}
                      className={cn('version-label', on(itemTeam) && 'on')}
                      data-team={itemTeam}
                    >
                      {versionOf(itemTeam, pt.build)}
                    </text>
                  </Fragment>,
                ];
              })}
            </Fragment>
          );
        })}
      </g>
      <g className="bump-labels">
        {geom.paintOrder.map((itemTeam) => {
          const last = geom.paths.get(itemTeam)?.at(-1);
          if (!last) {
            return null;
          }
          return (
            <g
              key={itemTeam}
              transform={`translate(${geom.margin.left + geom.inner.w + AXIS_LABEL_OFFSET},${geom.y(last[1])})`}
              className={cn(on(itemTeam) && 'on')}
              data-team={itemTeam}
            >
              <circle r={4} style={paint.get(itemTeam)?.fill ?? MUTED_FILL} />
              <text x={8}>
                <title>{itemTeam}</title>
                {itemTeam.length > MAX_LABEL_CHARS
                  ? `${itemTeam.slice(0, LABEL_SLICE_LEN)}…`
                  : itemTeam}
              </text>
            </g>
          );
        })}
      </g>
    </>
  );
};

export { BumpPaint };
