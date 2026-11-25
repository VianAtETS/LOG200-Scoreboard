// Page analyse : une partie rejouée coup par coup avec la mobilité, le territoire et la région de chaque
// couleur, ses coups critiques et son point de bascule (lib/analysis.ts). Sans ?game=, la dernière partie.

import { AnalysisChart } from '@/components/analysis/analysis-chart';
import { Board } from '@/components/board/board';
import { FirstIcon, LastIcon, NextIcon, PrevIcon } from '@/components/icons/icons';
import { Masthead } from '@/components/masthead/masthead';
import { GameInfo } from '@/components/replay/replay-section';
import { useGetGame } from '@/dao/hooks/use-games';
import { useGetSummary } from '@/dao/hooks/use-ladder';
import { useSearchParam } from '@/hooks/use-search-param';
import { analyse, describe } from '@/lib/analysis';
import type { TSeries } from '@/lib/analysis';
import { signed, versionOf } from '@/lib/format';
import { RED, positions, squareName } from '@/lib/replay';
import { cn } from '@/lib/utils';
import { withProviders } from '@/providers/with-providers';
import type { TColor } from '@/types/games.types';
import { useCallback, useEffect, useMemo, useState } from 'react';

const COLORS = ['red', 'black'] as const;
// Le pion noir se fond dans le fond sombre : le texte reste lisible dans les deux thèmes.
const STROKE: Record<TColor, string> = { red: 'var(--color-pawn-red)', black: 'var(--color-text)' };
const MEASURES: { key: keyof TSeries; label: string; hint: string }[] = [
  { key: 'terr', label: 'Territoire', hint: "cases atteintes avant l'adversaire" },
  { key: 'mob', label: 'Mobilité', hint: 'cases atteignables en un coup' },
  { key: 'reg', label: 'Région', hint: 'cases encore accessibles' },
];
const number = (v: number) => v.toLocaleString('fr-CA', { maximumFractionDigits: 1 });

const AnalysePage = () => {
  const { data: summary, error: summaryError } = useGetSummary();
  const [gameParam] = useSearchParam('game');
  const row = gameParam ? { id: gameParam } : summary?.scopes['']?.recent.at(-1);
  const { data: game, error: gameError } = useGetGame(row);
  const builds = useMemo(() => new Map(Object.entries(summary?.builds ?? {})), [summary]);
  const version = useCallback(
    (team: string, build?: string) => versionOf(builds, team, build),
    [builds],
  );

  const steps = useMemo(() => (game ? positions(game) : []), [game]);
  const analysis = useMemo(() => (game ? analyse(steps, game) : null), [game, steps]);
  const crit = useMemo(() => [...(analysis?.crit.keys() ?? [])].sort((a, b) => a - b), [analysis]);
  const last = steps.length - 1;
  // null : la fin de la partie.
  const [chosen, setChosen] = useState<number | null>(null);
  const k = chosen === null ? last : Math.min(chosen, last);
  const go = useCallback((i: number) => setChosen(Math.max(0, Math.min(last, i))), [last]);
  const prevCrit = crit.findLast((c) => c < k);
  const nextCrit = crit.find((c) => c > k);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.target as Element).closest('input, select, textarea')) {
        return;
      }
      const action = {
        ArrowLeft: () => go(k - 1),
        ArrowRight: () => go(k + 1),
        ArrowUp: () => prevCrit !== undefined && go(prevCrit),
        ArrowDown: () => nextCrit !== undefined && go(nextCrit),
        Home: () => go(0),
        End: () => go(last),
      }[e.key];
      if (!action) {
        return;
      }
      e.preventDefault();
      action();
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [go, k, last, prevCrit, nextCrit]);

  // Temps des coups des équipes, après l'ouverture ; vide s'il manque ou ne correspond pas aux coups.
  const moveMs = game?.move_ms?.length === game?.moves.length ? (game?.move_ms ?? []) : [];
  const offset = steps.length - moveMs.length;
  const moverOf = (i: number): TColor =>
    steps[i].cells[steps[i].move!.to] === RED ? 'red' : 'black';
  const timed = moveMs.map((ms, j) => ({ ms, at: offset + j, color: moverOf(offset + j) }));
  // Mesures des bots (lignes ARENA), chacune à la position que son coup produit.
  const firstMove = steps.length - (game?.moves.length ?? 0);
  const measured = COLORS.flatMap((color) => {
    const at = steps
      .map((_, i) => i)
      .filter((i) => i >= firstMove && i > 0 && moverOf(i) === color);
    return (game?.measures?.[color] ?? []).map((m, j) => {
      const ms = game?.move_ms?.[at[j] - firstMove];
      return { color, at: at[j], depth: m.depth, rate: ms ? (1000 * m.nodes) / ms : undefined };
    });
  });
  const depths = measured.flatMap((m) =>
    m.depth === undefined ? [] : [{ color: m.color, at: m.at, value: m.depth }],
  );
  const rates = measured.flatMap((m) =>
    m.rate === undefined ? [] : [{ color: m.color, at: m.at, value: m.rate }],
  );

  const failure = gameError ?? (gameParam ? null : summaryError);
  const step = steps[k];
  const name = (color: TColor) =>
    game ? `${game[color].team} (${color === 'red' ? 'rouge' : 'noir'})` : '';

  return (
    <>
      <Masthead current="analyse" />
      <main className="layout analyse-layout">
        <section className="section analyse-head" aria-labelledby="analyse-title">
          <h2 id="analyse-title">Analyse</h2>
          <p className="replay-info">
            {failure ? (
              `Partie indisponible : ${failure.message}`
            ) : game ? (
              <GameInfo game={game} versionOf={version} />
            ) : row || !summary ? (
              'Chargement de la partie…'
            ) : (
              'Aucune partie à analyser.'
            )}
          </p>
        </section>

        {game && analysis && step && (
          <>
            <section className="section analyse-board" aria-label="Position analysée">
              <div className="board-wrap">
                <Board step={step} index={k} label={`Position après le coup ${k} sur ${last}`} />
              </div>
              <div className="replay-controls">
                <button
                  className="icon"
                  type="button"
                  aria-label="Coup critique précédent"
                  disabled={prevCrit === undefined}
                  onClick={() => go(prevCrit!)}
                >
                  <FirstIcon />
                </button>
                <button
                  className="icon"
                  type="button"
                  aria-label="Coup précédent"
                  disabled={k === 0}
                  onClick={() => go(k - 1)}
                >
                  <PrevIcon />
                </button>
                <button
                  className="icon"
                  type="button"
                  aria-label="Coup suivant"
                  disabled={k === last}
                  onClick={() => go(k + 1)}
                >
                  <NextIcon />
                </button>
                <button
                  className="icon"
                  type="button"
                  aria-label="Coup critique suivant"
                  disabled={nextCrit === undefined}
                  onClick={() => go(nextCrit!)}
                >
                  <LastIcon />
                </button>
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
                    {COLORS.map((c) => (
                      <th key={c} scope="col">
                        {name(c)}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {MEASURES.map((m) => (
                    <tr key={m.key}>
                      <th scope="row" title={m.hint}>
                        {m.label}
                      </th>
                      {COLORS.map((c) => {
                        const values = analysis[c][m.key];
                        const delta = k ? values[k] - values[k - 1] : 0;
                        return (
                          <td key={c}>
                            {values[k]}
                            {delta !== 0 && (
                              <em className={delta > 0 ? 'up' : 'down'}> {signed(delta)}</em>
                            )}
                          </td>
                        );
                      })}
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>

            <section className="section analyse-moves" aria-labelledby="moves-title">
              <h2 id="moves-title">Coups</h2>
              <p className="analyse-note">
                <span className="crit-fine">▲</span> a gagné du territoire,{' '}
                <span className="crit-bad">▼</span> en a coûté, <span className="crit-tip">⚑</span>{' '}
                point de bascule : après lui, le perdant ne reprend jamais l'avantage.
              </p>
              <ol className="move-list">
                {steps.map((s, i) =>
                  s.move ? (
                    <li key={i}>
                      <button
                        type="button"
                        className={cn(i === k && 'current', s.opening && 'opening')}
                        aria-current={i === k}
                        onClick={() => go(i)}
                      >
                        <span className="move-number">{i}</span>
                        <i
                          className={cn('pawn', moverOf(i))}
                          aria-label={moverOf(i) === 'red' ? 'rouge' : 'noir'}
                        />
                        {`${squareName(s.move.from)} → ${squareName(s.move.to)} ✕ ${squareName(s.move.destroyed)}`}
                        {analysis.crit.get(i) === 'fine' && (
                          <span className="crit-fine" title="A gagné du territoire">
                            {' '}
                            ▲
                          </span>
                        )}
                        {analysis.crit.get(i) === 'bad' && (
                          <span className="crit-bad" title="A coûté du territoire">
                            {' '}
                            ▼
                          </span>
                        )}
                        {analysis.tip === i && (
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

            <section className="section analyse-charts" aria-labelledby="charts-title">
              <h2 id="charts-title">Au fil de la partie</h2>
              <p className="analyse-legend">
                {COLORS.map((c) => (
                  <span key={c} className="legend-line" style={{ color: STROKE[c] }}>
                    <span>{name(c)}</span>
                  </span>
                ))}
                <span className="legend-mark chart-cursor">position affichée</span>
                {crit.length > 0 && <span className="legend-mark chart-crit">coups critiques</span>}
                {analysis.tip !== null && <span className="legend-mark chart-tip">bascule</span>}
              </p>
              {MEASURES.map((m) => (
                <AnalysisChart
                  key={m.key}
                  title={`${m.label} : ${m.hint}`}
                  lines={COLORS.map((c) => ({ color: STROKE[c], values: analysis[c][m.key] }))}
                  last={last}
                  cursor={k}
                  crit={crit}
                  tip={analysis.tip}
                  onPick={go}
                />
              ))}
              {timed.length > 0 && (
                <AnalysisChart
                  title={`Temps par coup (ms)${game.time_ms ? `, ${game.time_ms} ms alloués` : ''}`}
                  bars={timed.map((t) => ({ color: STROKE[t.color], value: t.ms, at: t.at }))}
                  reference={game.time_ms}
                  last={last}
                  cursor={k}
                  crit={crit}
                  tip={analysis.tip}
                  onPick={go}
                />
              )}
              {[
                { title: 'Profondeur terminée par le bot', bars: depths },
                { title: 'Nœuds par seconde du bot', bars: rates },
              ].map(
                ({ title, bars }) =>
                  bars.length > 0 && (
                    <AnalysisChart
                      key={title}
                      title={title}
                      bars={bars.map((b) => ({ ...b, color: STROKE[b.color] }))}
                      last={last}
                      cursor={k}
                      crit={crit}
                      tip={analysis.tip}
                      onPick={go}
                    />
                  ),
              )}
            </section>

            <section className="section analyse-stats" aria-labelledby="stats-title">
              <h2 id="stats-title">Statistiques</h2>
              <div className="analyse-scroll">
                <table className="analyse-table">
                  <thead>
                    <tr>
                      <th scope="col">Mesure</th>
                      <th scope="col">Couleur</th>
                      {['Min', 'Moyenne', 'Médiane', 'Max', 'Écart type', 'N'].map((h) => (
                        <th key={h} scope="col">
                          {h}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {[
                      ...(timed.length > 0
                        ? [
                            {
                              label: 'Temps par coup (ms)',
                              of: (c: TColor) =>
                                timed.filter((t) => t.color === c).map((t) => t.ms),
                            },
                          ]
                        : []),
                      ...MEASURES.map((m) => ({
                        label: m.label,
                        of: (c: TColor) => analysis[c][m.key],
                      })),
                      ...(depths.length > 0
                        ? [
                            {
                              label: 'Profondeur du bot',
                              of: (c: TColor) =>
                                depths.filter((d) => d.color === c).map((d) => d.value),
                            },
                          ]
                        : []),
                      ...(rates.length > 0
                        ? [
                            {
                              label: 'Nœuds par seconde',
                              of: (c: TColor) =>
                                rates.filter((r) => r.color === c).map((r) => r.value),
                            },
                          ]
                        : []),
                    ].flatMap(({ label, of }) =>
                      COLORS.map((c) => {
                        const d = describe(of(c));
                        return (
                          <tr key={`${label}-${c}`}>
                            {c === 'red' && (
                              <th scope="rowgroup" rowSpan={2}>
                                {label}
                              </th>
                            )}
                            <td>{name(c)}</td>
                            {d ? (
                              [d.min, d.mean, d.median, d.max, d.sd, d.n].map((v, i) => (
                                <td key={i}>{number(v)}</td>
                              ))
                            ) : (
                              <td colSpan={6}>–</td>
                            )}
                          </tr>
                        );
                      }),
                    )}
                  </tbody>
                </table>
              </div>
            </section>
          </>
        )}
      </main>
    </>
  );
};

export default withProviders(AnalysePage);
