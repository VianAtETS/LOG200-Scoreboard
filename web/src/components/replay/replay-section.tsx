import { NodeBadge, RemoteBadge } from '@/components/badge/badge';
import { Board } from '@/components/board/board';
import { useDescriptor } from '@/dao/hooks/use-descriptor';
import { FirstIcon, LastIcon, NextIcon, PauseIcon, PlayIcon, PrevIcon } from '@/components/icons/icons';
import { squareName } from '@/lib/replay';
import { sideLabel } from '@/lib/game';
import { pageUrl } from '@/lib/url';
import { cn } from '@/lib/utils';
import type { TColor, TGame } from '@/types/games.types';
import type { TLiveGame } from '@/types/live.types';
import { useEffect, useState, type CSSProperties, type FC } from 'react';
import { endText } from './replay.data';
import type { TReplay, TReplayActions } from './use-replay';

const SHARE = 'Copier le lien';

type Props = {
  replay: TReplay;
  actions: TReplayActions;
  versionOf: (team: string, build?: string) => string;
};

const ReplaySection: FC<Props> = ({ replay, actions, versionOf }) => {
  const [shareLabel, setShareLabel] = useState(SHARE);
  const descriptor = useDescriptor();
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

  const size = `Plateau ${descriptor.rows} × ${descriptor.cols}`;
  let moveCount = '';
  let moveText = null;
  let boardLabel = size;
  if (step?.text) {
    // Les coups des clients sont les derniers pas, après l'ouverture.
    const ms = moveMs[index - (steps.length - moveMs.length)];
    const detail = step.opening ? ', ouverture' : ms !== undefined ? `, ${ms} ms` : '';
    moveCount = `Coup ${index} sur ${total}${detail}`;
    if (step.move) {
      // Isolation : le coup dit tout, départ, arrivée et case détruite.
      const { from, to, destroyed } = step.move;
      moveText = (
        <>
          {`${squareName(from)} → ${squareName(to)} `}
          <span className="destroyed">détruit {squareName(destroyed)}</span>
        </>
      );
      boardLabel = `Coup ${index} sur ${total} : ${squareName(from)} vers ${squareName(to)}, ${squareName(destroyed)} détruite`;
    } else {
      moveText = step.text;
      boardLabel = `Coup ${index} sur ${total} : ${step.text}`;
    }
  } else if (step) {
    moveCount = `Position initiale, ${total} coups`;
    boardLabel = `${size}, position initiale`;
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
        {game && descriptor.game === 'isolation' && (
          <a className="share analyse-link" href={pageUrl('analyse.html', { game: game.id })}>
            Analyser
          </a>
        )}
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
        </div>
        <input
          id="step"
          type="range"
          min="0"
          max={String(Math.max(0, total))}
          value={String(index)}
          style={{ '--progress': `${total > 0 ? (index / total) * 100 : 0}%` } as CSSProperties}
          aria-label="Coup affiché"
          onChange={(e) => actions.browse(+e.target.value)}
        />
      </div>
    </section>
  );
};

const GameInfo: FC<{ game: TGame; versionOf: Props['versionOf'] }> = ({ game: g, versionOf }) => {
  const descriptor = useDescriptor();
  const side = (color: TColor) => sideLabel(descriptor, color).toLowerCase();
  const ending = endText(g.winner, g.reason, { red: g.red.team, black: g.black.team }, descriptor);
  const player = (color: TColor) => (
    <>
      <strong>{[g[color].team, versionOf(g[color].team, g[color].build)].filter(Boolean).join(' ')}</strong>
      {g[color].remote && <RemoteBadge />}
    </>
  );
  return (
    <>
      {player('red')}
      {` (${side('red')}) contre `}
      {player('black')}
      {` (${side('black')}) : ${ending}.`}
      {g.node && <NodeBadge />}
    </>
  );
};

const LiveInfo: FC<{ game: TLiveGame }> = ({ game: g }) => {
  const descriptor = useDescriptor();
  const { winner, reason, cancelled, interrupted } = g.ended ?? {};
  const state = !g.ended
    ? 'en direct'
    : cancelled
      ? 'partie annulée'
      : interrupted
        ? 'direct interrompu'
        : endText(winner, reason!, { red: g.red, black: g.black }, descriptor);
  const side = (color: TColor) => sideLabel(descriptor, color).toLowerCase();
  return (
    <>
      <strong>{g.red}</strong>
      {` (${side('red')}) contre `}
      <strong>{g.black}</strong>
      {` (${side('black')}) : ${state}.`}
    </>
  );
};

export { GameInfo, ReplaySection };
