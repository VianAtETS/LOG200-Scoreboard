import { SERVER } from '@/dao/constants/api.const';
import { connectLive, liveUrl } from '@/lib/live';
import type { TLiveGames, TLiveState } from '@/types/live.types';
import { useEffect, useState } from 'react';

// Parties en cours, coup par coup, par le WebSocket /api/live. Se connecte une fois `enabled` vrai.
const useLiveGames = (enabled = true) => {
  const [live, setLive] = useState<{ games: TLiveGames; state: TLiveState }>({
    games: new Map(),
    state: 'connecting',
  });

  useEffect(() => {
    if (!enabled) {
      return;
    }
    return connectLive(liveUrl(SERVER, location.href), (games, state) => setLive({ games, state }));
  }, [enabled]);

  return live;
};

export { useLiveGames };
