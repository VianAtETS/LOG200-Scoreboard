import { Board } from '@/components/board/board';
import { squareName } from '@/lib/replay';
import { pageUrl } from '@/lib/url';
import { useCallback, useEffect, useState } from 'react';
import type { FC } from 'react';
import { GameInfo } from './game-info';
import { LiveInfo } from './live-info';
import { ReplayControls } from './replay-controls';
import type { TReplay, TReplayActions } from './use-replay';

const SHARE = 'Copier le lien';
const SHARE_RESET_MS = 2000;

type Props = {
  replay: TReplay;
  actions: TReplayActions;
  versionOf: (team: string, build?: string) => string;
};

function describePosition(replay: TReplay): {
  moveCount: string;
  moveText: React.ReactNode;
  boardLabel: string;
} {
  const { steps, moveMs, index } = replay;
  const step = steps[index];
  const total = steps.length - 1;
  let moveCount = '';
  let moveText = null;
  let boardLabel = 'Plateau 12 × 12';
  if (step?.move) {
    const { from, to, destroyed } = step.move;
    // Les coups des clients sont les derniers pas, après l'ouverture.
    const ms = moveMs[index - (steps.length - moveMs.length)];
    let detail = '';
    if (step.opening) {
      detail = ', ouverture';
    } else if (ms !== undefined) {
      detail = `, ${ms} ms`;
    }
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

  return { moveCount, moveText, boardLabel };
}

const ReplaySection: FC<Props> = ({ replay, actions, versionOf }) => {
  const [shareLabel, setShareLabel] = useState(SHARE);
  const { steps, index, game, live, message } = replay;
  const step = steps[index];
  const total = steps.length - 1;
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent): void => {
      const { target } = event;
      const queue = document.querySelector<HTMLDialogElement>('#queue');
      if (
        (target instanceof Element &&
          target.closest(
            'input, select, textarea, [contenteditable], [role="slider"], .ranking-scroll',
          )) ||
        queue?.open
      ) {
        return;
      }
      if (event.key === 'ArrowLeft') {
        actions.browse(actions.current().index - 1);
      }
      if (event.key === 'ArrowRight') {
        actions.browse(actions.current().index + 1);
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [actions]);
  const handleShare = useCallback(async (): Promise<void> => {
    if (!actions.pin()) {
      return;
    }
    try {
      await navigator.clipboard.writeText(location.href);
      setShareLabel('Lien copié');
    } catch {
      setShareLabel("Copiez l'adresse de la page");
    }
    setTimeout(() => setShareLabel(SHARE), SHARE_RESET_MS);
  }, [actions]);
  const { moveCount, moveText, boardLabel } = describePosition(replay);
  let info: React.ReactNode = 'Choisissez une partie pour la rejouer.';
  if (message !== null) {
    info = message;
  } else if (live) {
    info = <LiveInfo game={live} />;
  } else if (game) {
    info = <GameInfo game={game} versionOf={versionOf} />;
  }
  const handleShareClick = useCallback((): void => {
    void handleShare();
  }, [handleShare]);
  return (
    <section className="replay" aria-labelledby="replay-title">
      <div className="replay-head">
        <h2 id="replay-title">Replay</h2>
        <p id="replay-info" className="replay-info">
          {info}
        </p>
        {game && (
          <a className="share analyse-link" href={pageUrl('analyse.html', { game: game.id })}>
            Analyser
          </a>
        )}
        <button
          id="share"
          className="share"
          type="button"
          aria-live="polite"
          onClick={handleShareClick}
        >
          {shareLabel}
        </button>
      </div>
      <div className="board-wrap">
        {step ? (
          <Board id="board" step={step} index={index} label={boardLabel} />
        ) : (
          /* oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- The empty board uses the same styled container and accessible name as the rendered board. */
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
        <ReplayControls replay={replay} actions={actions} total={total} />
      </div>
    </section>
  );
};

export { GameInfo } from './game-info';

export { ReplaySection };
