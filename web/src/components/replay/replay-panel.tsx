import { NodeBadge, RemoteBadge } from '@/components/badge/badge';
import { Board } from '@/components/board/board';
import { FirstIcon, LastIcon, NextIcon, PauseIcon, PlayIcon, PrevIcon } from '@/components/icons/icons';
import { squareName } from '@/lib/replay';
import { cn } from '@/lib/utils';
import type { TColor, TGame } from '@/types/games.types';
import type { TLiveGame } from '@/types/live.types';
import { useEffect, useState, type FC } from 'react';
import { ENDINGS } from './replay.data';
import type { TReplay, TReplayActions } from './use-replay';

const SHARE = 'Copier le lien';

type Props = {
  replay: TReplay;
  actions: TReplayActions;
  versionOf: (team: string, build?: string) => string;
};

const ReplayPanel: FC<Props> = ({ replay, actions, versionOf }) => {
  const [shareLabel, setShareLabel] = useState(SHARE);
  const { steps, moveMs, index, game, live, message } = replay;
  const step = steps[index];
  const total = steps.length - 1;

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const target = e.target as Element;
      const queue = document.querySelector<HTMLDialogElement>('#queue');
      if (target.closest('input, select, textarea, [contenteditable], [role="slider"], .ranking-scroll') || queue?.open) return;
      if (e.key === 'ArrowLeft') actions.browse(actions.current().index - 1);
      if (e.key === 'ArrowRight') actions.browse(actions.current().index + 1);
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [actions]);

  const share = async () => {
    if (!actions.pin()) return;
    try {
      await navigator.clipboard.writeText(location.href);
      setShareLabel('Lien copié');
    } catch {
      setShareLabel("Copiez l'adresse de la page");
    }
    setTimeout(() => setShareLabel(SHARE), 2000);
  };

  let moveCount = '';
  let moveText = null;
  let boardLabel = 'Plateau 12 × 12';
  if (step?.move) {
    const { from, to, destroyed } = step.move;
    // Les coups des clients sont les derniers pas, après l'ouverture.
    const ms = moveMs[index - (steps.length - moveMs.length)];
    const detail = step.opening ? ', ouverture' : ms !== undefined ? `, ${ms} ms` : '';
    moveCount = `Coup ${index} sur ${total}${detail}`;
    moveText = (
      <>
        {`${squareName(from)} → ${squareName(to)} `}
        <span className="destroyed">détruit {squareName(destroyed)}</span>
      </>
    );
    boardLabel = `Coup ${index} sur ${total} : ${squareName(from)} vers ${squareName(to)}, ${squareName(destroyed)} détruite`;
  } else if (step) {
    moveCount = `Position initiale, ${total} coups`;
    boardLabel = 'Plateau 12 × 12, position initiale';
  }

  return (
    <section className="replay" aria-labelledby="replay-title">
      <div className="replay-head">
        <h2 id="replay-title">Replay</h2>
        <p id="replay-info" className="replay-info">
          {message ??
            (live ? (
              <LiveInfo game={live} />
            ) : game ? (
              <GameInfo game={game} versionOf={versionOf} />
            ) : (
              'Choisissez une partie pour la rejouer.'
            ))}
        </p>
        <button id="share" className="share" type="button" aria-live="polite" onClick={share}>
          {shareLabel}
        </button>
      </div>
      <div className="board-wrap">
        {step ? (
          <Board id="board" step={step} index={index} label={boardLabel} />
        ) : (
          <div id="board" className="board" role="img" aria-label={boardLabel} />
        )}
      </div>
      <div className="replay-bar">
        <p className="move" aria-live="polite">
          <span id="move-count" className="move-count">
            {moveCount}
          </span>
          <span id="move-text" className="move-text">
            {moveText}
          </span>
        </p>
        <div className="replay-controls">
          <button id="first" className="icon" type="button" aria-label="Début de la partie" onClick={() => actions.browse(0)}>
            <FirstIcon />
          </button>
          <button id="prev" className="icon" type="button" aria-label="Coup précédent" onClick={() => actions.browse(index - 1)}>
            <PrevIcon />
          </button>
          <button
            id="play"
            className={cn('icon play', replay.playing && 'playing')}
            type="button"
            aria-label={replay.playing ? 'Pause' : 'Lecture'}
            onClick={actions.togglePlayback}
          >
            <PlayIcon className="i-play" />
            <PauseIcon className="i-pause" />
          </button>
          <button id="next" className="icon" type="button" aria-label="Coup suivant" onClick={() => actions.browse(index + 1)}>
            <NextIcon />
          </button>
          <button id="last" className="icon" type="button" aria-label="Fin de la partie" onClick={() => actions.browse(total)}>
            <LastIcon />
          </button>
          <input
            id="step"
            type="range"
            min="0"
            max={String(Math.max(0, total))}
            value={String(index)}
            aria-label="Coup affiché"
            onChange={(e) => actions.browse(+e.target.value)}
          />
        </div>
      </div>
    </section>
  );
};

const GameInfo: FC<{ game: TGame; versionOf: Props['versionOf'] }> = ({ game: g, versionOf }) => {
  const loser = g.winner === 'red' ? 'black' : 'red';
  const ending = ENDINGS[g.reason]?.(g[loser].team) ?? g.reason;
  const player = (color: TColor) => (
    <>
      <strong>{[g[color].team, versionOf(g[color].team, g[color].build)].filter(Boolean).join(' ')}</strong>
      {g[color].remote && <RemoteBadge />}
    </>
  );
  return (
    <>
      {player('red')}
      {' (rouge) contre '}
      {player('black')}
      {` (noir) : ${g[g.winner].team} gagne, ${ending}.`}
      {g.node && <NodeBadge />}
    </>
  );
};

const LiveInfo: FC<{ game: TLiveGame }> = ({ game: g }) => {
  const { winner, reason, cancelled, interrupted } = g.ended ?? {};
  const loser = winner === 'red' ? g.black : g.red;
  const state = !g.ended
    ? 'en direct'
    : cancelled
      ? 'partie annulée'
      : interrupted
        ? 'direct interrompu'
        : `${g[winner!]} gagne, ${ENDINGS[reason!]?.(loser) ?? reason}`;
  return (
    <>
      <strong>{g.red}</strong>
      {' (rouge) contre '}
      <strong>{g.black}</strong>
      {` (noir) : ${state}.`}
    </>
  );
};

export { ReplayPanel };
