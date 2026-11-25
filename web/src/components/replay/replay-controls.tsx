import {
  FirstIcon,
  LastIcon,
  NextIcon,
  PauseIcon,
  PlayIcon,
  PrevIcon,
} from '@/components/icons/icons';
import { cn } from '@/lib/utils';
import { useCallback, useMemo } from 'react';
import type { CSSProperties, ChangeEvent, FC } from 'react';
import type { TReplay, TReplayActions } from './use-replay';

const ReplayControls: FC<{ replay: TReplay; actions: TReplayActions; total: number }> = ({
  replay,
  actions,
  total,
}) => {
  const { index } = replay;
  const handleFirst = useCallback((): void => actions.browse(0), [actions]);
  const handlePrev = useCallback((): void => actions.browse(index - 1), [actions, index]);
  const handleNext = useCallback((): void => actions.browse(index + 1), [actions, index]);
  const handleLast = useCallback((): void => actions.browse(total), [actions, total]);
  const handleBrowse = useCallback(
    (event: ChangeEvent<HTMLInputElement>): void => actions.browse(Number(event.target.value)),
    [actions],
  );
  const handlePlay = actions.togglePlayback;
  const style = useMemo<CSSProperties & { '--progress': string }>(
    () => ({ '--progress': `${total > 0 ? (index / total) * 100 : 0}%` }),
    [index, total],
  );
  return (
    <>
      <div className="replay-controls">
        <button
          id="first"
          className="icon"
          type="button"
          aria-label="Début de la partie"
          onClick={handleFirst}
        >
          <FirstIcon />
        </button>
        <button
          id="prev"
          className="icon"
          type="button"
          aria-label="Coup précédent"
          onClick={handlePrev}
        >
          <PrevIcon />
        </button>
        <button
          id="play"
          className={cn('icon play', replay.playing && 'playing')}
          type="button"
          aria-label={replay.playing ? 'Pause' : 'Lecture'}
          onClick={handlePlay}
        >
          {/* oxlint-disable-next-line react/forbid-component-props -- The class selects the active playback glyph. */}
          <PlayIcon className="i-play" />
          {/* oxlint-disable-next-line react/forbid-component-props -- The class selects the active playback glyph. */}
          <PauseIcon className="i-pause" />
        </button>
        <button
          id="next"
          className="icon"
          type="button"
          aria-label="Coup suivant"
          onClick={handleNext}
        >
          <NextIcon />
        </button>
        <button
          id="last"
          className="icon"
          type="button"
          aria-label="Fin de la partie"
          onClick={handleLast}
        >
          <LastIcon />
        </button>
      </div>
      <input
        id="step"
        type="range"
        min="0"
        max={String(Math.max(0, total))}
        value={String(index)}
        style={style}
        aria-label="Coup affiché"
        onChange={handleBrowse}
      />
    </>
  );
};

export { ReplayControls };
