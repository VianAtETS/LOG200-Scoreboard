import type { TRankPoint } from '@/lib/bump';
import { cn } from '@/lib/utils';
import type { TRatingRow } from '@/types/games.types';
import { useMemo, useRef } from 'react';
import type { FC } from 'react';
import { chartSummary, gameUrl, matchOutcome, outcomeText, valueText } from './bump-summary';
import { BumpTooltip } from './bump-tooltip';
import { BumpPaint } from './bump-paint';
import { useChartGeometry } from './use-chart-geometry';
import { useBumpInteraction } from './use-bump-interaction';
import type { TChartMetric } from './bump.utils';

type Props = {
  columns: TRatingRow[];
  series: Map<string, TRankPoint[]>;
  selected: string[];
  metric: TChartMetric;
  palette: Map<string, string>;
  versionOf: (team: string, build?: string) => string;
  loading?: boolean;
};

const BumpChart: FC<Props> = ({
  columns,
  series,
  selected,
  metric,
  palette,
  versionOf,
  loading = false,
}) => {
  const plot = useRef<HTMLDivElement>(null);
  const { geom, lines, size } = useChartGeometry(plot, columns.length, series, selected, metric);
  const {
    figure,
    slider,
    inspection,
    hide,
    move,
    down,
    key,
    focus: openSelection,
    leave,
    blur,
    contextMenu,
  } = useBumpInteraction(plot, geom, columns, series, metric);
  const col = inspection?.col ?? null;
  const rows = useMemo(() => inspection?.rows ?? [], [inspection]);
  const team = inspection?.team ?? null;
  const game = col === null ? null : columns[col];
  const { winner, loser, matchup } = matchOutcome(game ?? null);
  const result = outcomeText(winner, loser, matchup);
  const left = geom && col !== null ? geom.x(col) : 0;

  const summary = chartSummary({ loading, geom, selected, columns, metric, series });

  const cursorStyle = useMemo(
    () =>
      geom
        ? { left: `${left}px`, top: `${geom.margin.top}px`, height: `${geom.inner.h}px` }
        : undefined,
    [geom, left],
  );

  return (
    // oxlint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- Close only when focus leaves the figure, including the tooltip button.
    <figure ref={figure} className="bump-figure" onPointerLeave={leave} onBlur={blur}>
      <div
        ref={plot}
        id="bump-plot"
        className="bump-plot"
        onPointerMove={move}
        onPointerDown={down}
        onContextMenu={contextMenu}
      >
        <svg
          id="bump-svg"
          className={cn(col !== null && team !== null && 'focusing')}
          ref={slider}
          onFocus={openSelection}
          onKeyDown={key}
          // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- A native range input cannot expose the chart’s game and team navigation.
          role="slider"
          tabIndex={0}
          aria-valuemin={0}
          aria-valuemax={Math.max(0, columns.length - 1)}
          aria-valuenow={col ?? 0}
          aria-valuetext={valueText(game, team, rows, result)}
          aria-describedby="bump-summary"
          aria-label="Parties : flèches gauche et droite pour parcourir, haut et bas pour changer d'équipe, Entrée pour rejouer"
          viewBox={`0 0 ${size.width} ${size.height}`}
        >
          {geom && (
            <BumpPaint
              geom={geom}
              columns={columns}
              lines={lines}
              palette={palette}
              team={team}
              versionOf={versionOf}
              series={series}
            />
          )}
        </svg>
        {loading && (
          <div className="bump-loading" aria-hidden="true">
            <span className="spinner" />
          </div>
        )}
        <div id="bump-cursor" className="bump-cursor" hidden={col === null} style={cursorStyle} />
        <BumpTooltip
          game={game ?? null}
          col={col}
          count={columns.length}
          rows={rows}
          team={team}
          palette={palette}
          left={left}
          geom={geom}
          winner={winner}
          matchup={matchup}
          loser={loser}
          versionOf={versionOf}
          hide={hide}
        />
      </div>
      <figcaption id="bump-summary" className="bump-summary">
        {summary}
        {/* Toujours rendu pour éviter de faire sauter l'interface. */}
        <a
          className={cn('graph-link bump-replay', !game && 'idle')}
          href={game ? gameUrl(game.id) : undefined}
        >
          Rejouer la partie sélectionnée
        </a>
      </figcaption>
    </figure>
  );
};

export { BumpChart };
