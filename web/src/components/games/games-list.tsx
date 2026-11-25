import { useScrollEdges } from '@/hooks/use-scroll-edges';
import { cn } from '@/lib/utils';
import type { TGameRow, TRatingChange, TSide } from '@/types/games.types';
import { useRef } from 'react';
import type { FC } from 'react';
import { GameItem } from './game-item';
import type { TSideOrder } from './game-item';

const RECENT_GAMES = 50;

type Props = {
  games: TGameRow[];
  rating: (side: TSide) => TRatingChange;
  currentId: string | null;
  selectedTeam?: string;
  sideOrder?: TSideOrder;
  onSelect: (game: TGameRow) => void;
};

const GamesList: FC<Props> = ({ games, rating, currentId, selectedTeam, sideOrder, onSelect }) => {
  const ref = useRef<HTMLOListElement>(null);
  const scroll = useScrollEdges(ref);
  const items = games.slice(-RECENT_GAMES).reverse();

  return (
    <ol
      ref={ref}
      id="games"
      className={cn('scroll games-list', scroll.edges.less && 'less', scroll.edges.more && 'more')}
      tabIndex={-1}
      onScroll={scroll.onScroll}
    >
      {items.length > 0 ? (
        items.map((g) => (
          <GameItem
            key={g.id}
            game={g}
            rating={rating}
            current={g.id === currentId}
            selectedTeam={selectedTeam}
            sideOrder={sideOrder}
            onSelect={onSelect}
          />
        ))
      ) : (
        <li className="empty">Les parties jouées apparaîtront ici.</li>
      )}
    </ol>
  );
};

export { GamesList };
