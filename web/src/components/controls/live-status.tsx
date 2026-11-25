import { cn } from '@/lib/utils';
import type { FC } from 'react';

export type TLiveDot = 'on' | 'error' | '';

type Props = {
  dot: TLiveDot;
  text: string;
  // Nombre d'arrivées de données (usePulse) : le point pulse à chacune.
  pulses?: number;
};

const LiveStatus: FC<Props> = ({ dot, text, pulses = 0 }) => {
  return (
    <p className="status" role="status">
      {/* Une nouvelle clé remonte le point : l'animation repart. */}
      <span
        key={pulses}
        id="live"
        className={cn('live', dot, pulses > 0 && 'pulse')}
        aria-hidden="true"
      />
      <span id="status">{text}</span>
    </p>
  );
};

export { LiveStatus };
