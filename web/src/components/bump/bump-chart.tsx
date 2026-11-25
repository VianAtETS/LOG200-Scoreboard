import { CloseIcon } from '@/components/icons/icons';
import { pointAt } from '@/lib/bump';
import type { TRankPoint } from '@/lib/bump';
import { dayFormat, fullFormat, ordinal } from '@/lib/format';
import { pageUrl } from '@/lib/url';
import { cn } from '@/lib/utils';
import type { TRatingRow } from '@/types/games.types';
import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import type { FC, KeyboardEvent } from 'react';
import { chartGeometry, dateTicks, valueAt } from './bump.utils';
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

type THover = { col: number | null; team: string | null };
type TPointerPosition = { clientX: number; clientY: number };

const muted = 'var(--color-text-muted)';

const BumpChart: FC<Props> = ({
  columns,
  series,
  selected,
  metric,
  palette,
  versionOf,
  loading = false,
}) => {
  const figure = useRef<HTMLElement>(null);
  const plot = useRef<HTMLDivElement>(null);
  const tip = useRef<HTMLDivElement>(null);
  const pointerFrame = useRef<number | null>(null);
  const pendingPoint = useRef<TPointerPosition | null>(null);
  const [size, setSize] = useState({ width: 240, height: 200 });
  const [hover, setHover] = useState<THover>({ col: null, team: null });

  useLayoutEffect(() => {
    const node = plot.current!;
    const observer = new ResizeObserver(() => {
      const box = node.getBoundingClientRect();
      setSize({ width: Math.max(240, box.width), height: Math.max(200, box.height) });
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  const geom = useMemo(
    () => chartGeometry(columns.length, series, selected, metric, size),
    [columns.length, series, selected, metric, size],
  );

  const lines = useMemo(() => {
    if (!geom) {
      return [];
    }
    const cached = new Set(
      [...geom.paths]
        .filter(([, path]) => path.length > geom.inner.w)
        .sort(([, a], [, b]) => b.length - a.length)
        .slice(0, 10)
        .map(([t]) => t),
    );
    return geom.paintOrder.map((t) => {
      const path = geom.paths.get(t)!;
      return {
        team: t,
        d: path.map(([c, value], i) => `${i ? 'L' : 'M'}${geom.x(c)},${geom.y(value)}`).join(''),
        cached: cached.has(t),
      };
    });
  }, [geom]);

  const cancelPointer = useCallback(() => {
    if (pointerFrame.current !== null) {
      cancelAnimationFrame(pointerFrame.current);
    }
    pointerFrame.current = null;
    pendingPoint.current = null;
  }, []);

  // Queued coordinates belong to the current geometry and column domain.
  useLayoutEffect(() => cancelPointer, [cancelPointer, geom, columns]);

  // Rang ou Elo exact, pas lissé, meilleure d'abord.
  const standingsAt = (col: number) =>
    (geom?.drawn ?? [])
      .map((team) => ({
        team,
        p: pointAt(series.get(team)!, col),
        before: pointAt(series.get(team)!, col - 1),
      }))
      .filter((r): r is { team: string; p: TRankPoint; before: TRankPoint | undefined } =>
        Boolean(r.p),
      )
      .sort((a, b) =>
        metric === 'rank' ? a.p.rank - b.p.rank : b.p.rating - a.p.rating || a.p.rank - b.p.rank,
      );

  const clampColumn = (col: number) => Math.max(0, Math.min(columns.length - 1, col));

  const inspectAt = (col: number, team: string | null) => {
    col = clampColumn(col);
    const rows = standingsAt(col);
    if (!rows.some((r) => r.team === team)) {
      team = rows[0]?.team ?? null;
    }
    return { col, team, rows };
  };

  const showAt = (col: number, team = hover.team) => {
    if (!geom) {
      return void hide();
    }
    cancelPointer();
    const next = inspectAt(col, team);
    setHover((h) =>
      h.col === next.col && h.team === next.team ? h : { col: next.col, team: next.team },
    );
  };

  const hide = () => {
    cancelPointer();
    setHover((h) => (h.col === null ? h : { ...h, col: null }));
  };

  const toolTipOpenned = hover.col !== null;

  useEffect(() => {
    if (!toolTipOpenned) {
      return;
    }
    const handlePointerDown = (e: PointerEvent) => {
      if (figure.current?.contains(e.target as Node)) {
        return;
      }
      cancelPointer();
      setHover((h) => ({ ...h, col: null }));
    };
    document.addEventListener('pointerdown', handlePointerDown);
    return () => document.removeEventListener('pointerdown', handlePointerDown);
  }, [toolTipOpenned, cancelPointer]);

  const pointed = (e: TPointerPosition) => {
    const box = plot.current!.getBoundingClientRect();
    const px = e.clientX - box.left;
    const py = e.clientY - box.top;
    const col = Math.round(((px - geom!.margin.left) / geom!.inner.w) * (columns.length - 1));
    const at = clampColumn(col);
    let team: string | null = null;
    let best = Infinity;
    for (const [t, path] of geom!.paths) {
      if (path[0][0] > at) {
        continue;
      }
      const distance = Math.abs(geom!.y(valueAt(path, at)) - py);
      if (distance < best) {
        [team, best] = [t, distance];
      }
    }
    return { col: at, team };
  };

  // Le lien garde les équipes choisies et les filtres du graphe.
  const gameUrl = (id: string) => pageUrl('./', { game: id });

  const handleKeyDown = (e: KeyboardEvent) => {
    if (!geom) {
      return;
    }
    const current = col ?? columns.length - 1;
    const page = Math.max(1, Math.round(columns.length / 10));
    const moves: Record<string, number> = {
      ArrowLeft: current - 1,
      ArrowRight: current + 1,
      PageUp: current - page,
      PageDown: current + page,
      Home: 0,
      End: columns.length - 1,
    };
    if (e.key in moves) {
      showAt(moves[e.key]);
    } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      const rows = standingsAt(current);
      const i = rows.findIndex((r) => r.team === hover.team) + (e.key === 'ArrowUp' ? -1 : 1);
      showAt(current, rows[Math.max(0, Math.min(rows.length - 1, i))]?.team ?? null);
    } else if (e.key === 'Enter' || e.key === ' ') {
      cancelPointer();
      location.assign(gameUrl(columns[current].id));
    } else if (e.key === 'Escape') {
      hide();
    } else {
      return;
    }
    e.preventDefault();
  };

  // Point visé, recalé sur le dessin du moment : il a pu changer depuis.
  const inspection = geom && hover.col !== null ? inspectAt(hover.col, hover.team) : null;
  const col = inspection?.col ?? null;
  const rows = inspection?.rows ?? [];
  const team = inspection?.team ?? null;
  const game = col === null ? null : columns[col];
  // Session deltas also describe tournament games, whose global deltas are both zero.
  const winner =
    game && game.red.session[1] !== game.black.session[1]
      ? game.red.session[1] > game.black.session[1]
        ? game.red.team
        : game.black.team
      : null;
  const loser = game ? (winner === game.red.team ? game.black.team : game.red.team) : '';
  const matchup = game ? `${game.red.team} contre ${game.black.team}` : '';
  const result = winner ? `${winner} bat ${loser}` : matchup;
  const left = geom && col !== null ? geom.x(col) : 0;

  useLayoutEffect(() => {
    const node = tip.current;
    if (!node || !geom || col === null) {
      return;
    }
    const width = node.offsetWidth;
    node.style.left = `${left + 14 + width <= geom.width ? left + 14 : Math.max(0, left - 14 - width)}px`;
  });

  let summary: string;
  if (loading) {
    summary = 'Chargement du graphe…';
  } else if (geom) {
    const from = dayFormat.format(new Date(columns[0].ts));
    const to = dayFormat.format(new Date(columns[columns.length - 1].ts));
    const parts = [
      `${metric === 'rank' ? 'Rang' : 'Elo'} de ${geom.drawn.length} équipes sur ${columns.length} parties, du ${from} au ${to}`,
    ];
    const teams = geom.drawn.filter((t) => !series.get(t)![0].bot);
    if (teams.length) {
      const value = (t: string) => series.get(t)!.at(-1)![metric];
      const leader = teams.reduce((a, b) =>
        (metric === 'rank' ? value(a) <= value(b) : value(a) >= value(b)) ? a : b,
      );
      parts.push(
        `en tête des équipes suivies : ${leader}, ${metric === 'rank' ? ordinal(value(leader)) : `${Math.round(value(leader))} Elo`}`,
      );
    }
    summary = `${parts.join(' ; ')}.`;
  } else {
    summary = !selected.length
      ? 'Aucune équipe choisie.'
      : 'Aucune partie de ces équipes dans cette fenêtre.';
  }

  const focus = rows.find((r) => r.team === team);
  const reading = focus
    ? `${team} ${ordinal(focus.p.rank)}, ${Math.round(focus.p.rating)} Elo, `
    : '';
  const on = (t: string) => t === team;

  return (
    <figure
      ref={figure}
      className="bump-figure"
      onPointerLeave={(e) => {
        if (!e.currentTarget.contains(document.activeElement)) {
          hide();
        }
      }}
      onBlur={(e) => {
        if (e.relatedTarget && !e.currentTarget.contains(e.relatedTarget)) {
          hide();
        }
      }}
    >
      <div
        ref={plot}
        id="bump-plot"
        className="bump-plot"
        role="slider"
        tabIndex={0}
        aria-valuemin={0}
        aria-valuemax={geom ? columns.length - 1 : undefined}
        aria-valuenow={col ?? undefined}
        aria-valuetext={
          game ? `${reading}${fullFormat.format(new Date(game.ts))}, ${result}` : undefined
        }
        aria-describedby="bump-summary"
        aria-label="Parties : flèches gauche et droite pour parcourir, haut et bas pour changer d'équipe, Entrée pour rejouer"
        onPointerMove={(e) => {
          if (!geom || !e.isPrimary) {
            return;
          }
          pendingPoint.current = { clientX: e.clientX, clientY: e.clientY };
          if (pointerFrame.current !== null) {
            return;
          }
          pointerFrame.current = requestAnimationFrame(() => {
            pointerFrame.current = null;
            const point = pendingPoint.current;
            pendingPoint.current = null;
            if (!point) {
              return;
            }
            const at = pointed(point);
            showAt(at.col, at.team);
          });
        }}
        onPointerDown={(e) => {
          if (
            !geom ||
            !e.isPrimary ||
            e.button !== 0 ||
            (e.target as Element).closest('.tip-dismiss')
          ) {
            return;
          }
          cancelPointer();
          e.currentTarget.focus({ preventScroll: true });
          e.currentTarget.setPointerCapture(e.pointerId);
          const at = pointed(e);
          showAt(at.col, at.team);
        }}
        onContextMenu={(e) => e.preventDefault()}
        onFocus={() => geom && showAt(hover.col ?? columns.length - 1)}
        onKeyDown={handleKeyDown}
      >
        <svg
          id="bump-svg"
          className={cn(col !== null && team && 'focusing')}
          aria-hidden="true"
          viewBox={`0 0 ${size.width} ${size.height}`}
        >
          {geom && (
            <>
              <g className="bump-grid">
                {geom.ticks.map((value) => (
                  <Fragment key={value}>
                    <line
                      x1={geom.margin.left}
                      x2={geom.margin.left + geom.inner.w}
                      y1={geom.y(value)}
                      y2={geom.y(value)}
                    />
                    <text x={geom.margin.left - 8} y={geom.y(value)} className="rank">
                      {value}
                    </text>
                  </Fragment>
                ))}
                {dateTicks(columns, geom.inner.w).map((t) => (
                  <text
                    key={t.col}
                    x={geom.x(t.col)}
                    y={geom.height - 8}
                    className="date"
                    textAnchor={t.anchor}
                  >
                    {t.text}
                  </text>
                ))}
              </g>
              {/* Changing stroke width rerasterizes dense paths during hover. */}
              <g className="bump-lines">
                {lines.map(({ team: t, d, cached }) => (
                  <g
                    key={t}
                    className={cn(
                      series.get(t)![0].bot && 'bot',
                      on(t) && 'on',
                      cached && 'cached',
                    )}
                    style={{ stroke: palette.get(t) ?? muted }}
                    data-team={t}
                  >
                    <path d={d} className="normal-stroke" />
                    <path d={d} className="focus-stroke" />
                  </g>
                ))}
              </g>
              <g className="bump-nodes">
                {geom.paintOrder.map((t) => {
                  const points = series.get(t)!;
                  const path = geom.paths.get(t)!;
                  const fill = { fill: palette.get(t) ?? muted };
                  return (
                    <Fragment key={t}>
                      {points[0].x > 0 && (
                        <circle
                          cx={geom.x(path[0][0])}
                          cy={geom.y(path[0][1])}
                          r={3}
                          className={cn(on(t) && 'on')}
                          style={fill}
                          data-team={t}
                        />
                      )}
                      {points
                        .filter((p) => p.version)
                        .map((p) => {
                          const cy = geom.y(valueAt(path, p.x));
                          return (
                            <Fragment key={p.x}>
                              <circle
                                cx={geom.x(p.x)}
                                cy={cy}
                                r={5}
                                className={cn('version', on(t) && 'on')}
                                style={fill}
                                data-team={t}
                              />
                              <text
                                x={geom.x(p.x)}
                                y={cy - 9}
                                className={cn('version-label', on(t) && 'on')}
                                data-team={t}
                              >
                                {versionOf(t, p.build)}
                              </text>
                            </Fragment>
                          );
                        })}
                    </Fragment>
                  );
                })}
              </g>
              <g className="bump-labels">
                {geom.paintOrder.map((t) => {
                  const path = geom.paths.get(t)!;
                  return (
                    <g
                      key={t}
                      transform={`translate(${geom.margin.left + geom.inner.w + 8},${geom.y(path.at(-1)[1])})`}
                      className={cn(on(t) && 'on')}
                      data-team={t}
                    >
                      <circle r={4} style={{ fill: palette.get(t) ?? muted }} />
                      <text x={8}>
                        <title>{t}</title>
                        {t.length > 11 ? `${t.slice(0, 10)}…` : t}
                      </text>
                    </g>
                  );
                })}
              </g>
            </>
          )}
        </svg>
        {loading && (
          <div className="bump-loading" aria-hidden="true">
            <span className="spinner" />
          </div>
        )}
        <div
          id="bump-cursor"
          className="bump-cursor"
          hidden={col === null}
          style={
            geom
              ? { left: `${left}px`, top: `${geom.margin.top}px`, height: `${geom.inner.h}px` }
              : undefined
          }
        />
        <div ref={tip} id="bump-tip" className="bump-tip" aria-hidden="true" hidden={!game}>
          {game && (
            <>
              <p className="tip-when">
                {`${fullFormat.format(new Date(game.ts))} · partie ${col! + 1} sur ${columns.length}`}
              </p>
              <p className="tip-game">
                {winner && <strong>{winner}</strong>}
                {winner ? ` bat ${loser}` : matchup}
              </p>
              <ol className="tip-rows">
                {rows.map(({ team: t, p, before }) => {
                  const move =
                    before && before.rank !== p.rank ? (p.rank < before.rank ? ' ▲' : ' ▼') : '';
                  const played = t === game.red.team || t === game.black.team;
                  return (
                    <li key={t} className={cn(t === team && 'on', played && 'played')}>
                      <span className="chip-dot" style={{ background: palette.get(t) ?? muted }} />
                      <span className="tip-rank">{`${ordinal(p.rank)}${move}`}</span>
                      <span className="tip-team">
                        {[t, p.bot ? 'bot' : versionOf(t, p.build)].filter(Boolean).join(' ')}
                      </span>
                      <span className="tip-elo">{Math.round(p.rating)} Elo</span>
                    </li>
                  );
                })}
              </ol>
            </>
          )}
          <button
            type="button"
            className="tip-dismiss"
            tabIndex={-1}
            aria-label="Fermer"
            onClick={hide}
          >
            <CloseIcon />
          </button>
        </div>
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
