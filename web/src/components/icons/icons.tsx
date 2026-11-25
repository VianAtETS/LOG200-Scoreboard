// Icônes de la page, en traits : leur couleur et leur épaisseur viennent du CSS.
import type { FC } from 'react';

type Props = { className?: string };

const makePathIcon = (path: string, viewBox = '0 0 24 24'): FC<Props> => {
  const Icon: FC<Props> = ({ className }) => (
    <svg viewBox={viewBox} aria-hidden="true" className={className}>
      <path d={path} />
    </svg>
  );
  return Icon;
};

export const CloseIcon = makePathIcon('M6 6l12 12M18 6L6 18');
export const MenuIcon = makePathIcon('M4 7h16M4 12h16M4 17h16');
export const ChevronDownIcon = makePathIcon('M6 9l6 6 6-6');
export const FirstIcon = makePathIcon('M6 5v14M19 5l-9 7 9 7z');
export const PrevIcon = makePathIcon('M16 5l-9 7 9 7z');
export const PlayIcon = makePathIcon('M8 5l11 7-11 7z');
export const PauseIcon = makePathIcon('M7.5 6h2v12h-2zM14.5 6h2v12h-2z');
export const NextIcon = makePathIcon('M8 5l9 7-9 7z');
export const LastIcon = makePathIcon('M18 5v14M5 5l9 7-9 7z');

// oxlint-disable-next-line react/no-multi-comp -- This catalog keeps related icons together.
export const LiveIcon: FC<Props> = ({ className }) => (
  <svg viewBox="0 0 24 24" aria-hidden="true" className={className}>
    <circle cx="12" cy="12" r="2.5" />
    <path d="M7.8 7.8a6 6 0 0 0 0 8.4M16.2 7.8a6 6 0 0 1 0 8.4M4.9 4.9a10 10 0 0 0 0 14.2M19.1 4.9a10 10 0 0 1 0 14.2" />
  </svg>
);

// oxlint-disable-next-line react/no-multi-comp -- This catalog keeps related icons together.
export const DuelIcon: FC<Props> = ({ className }) => (
  <svg className={className ?? 'duel-icon'} viewBox="0 0 20 16" aria-hidden="true">
    <path className="duel-sword red" d="M3 1l12 12m-2-1 3 3m-5-1 4-4" />
    <path className="duel-sword black" d="M17 1L5 13m2-1-3 3m5-1-4-4" />
  </svg>
);
