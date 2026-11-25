import { getGame } from '@/dao/scoreboard.repository';
import { positions } from '@/lib/replay';
import type { TStep } from '@/lib/replay';
import { searchParams, setParam } from '@/lib/url';
import type { TGame, TGameRow } from '@/types/games.types';
import type { TLiveGame } from '@/types/live.types';
import { useEffect, useMemo, useRef, useState } from 'react';
import { PLAY_MS } from './replay.data';

export type TReplay = {
  id: string | null;
  // Partie terminée affichée, ou partie suivie en direct.
  game: TGame | null;
  live: TLiveGame | null;
  steps: TStep[];
  // moveMs[k] : temps du coup moves[k], vide si absent ou incohérent.
  moveMs: number[];
  index: number;
  // La personne a choisi une partie (ou un lien ?game=) ; sinon le replay suit la dernière jouée.
  pinned: boolean;
  playing: boolean;
  // Remplace la ligne d'info : erreur de chargement, direct indisponible.
  message: string | null;
};

export type TReplayActions = {
  showGame: (row: Pick<TGameRow, 'id' | 'at'>, pinned?: boolean) => Promise<void>;
  showLive: (game: TLiveGame) => void;
  browse: (index: number) => void;
  togglePlayback: () => void;
  pin: () => boolean;
  unpin: () => void;
  setMessage: (message: string) => void;
  current: () => TReplay;
};

const initialReplay = (): TReplay => ({
  id: null,
  game: null,
  live: null,
  steps: [],
  moveMs: [],
  index: 0,
  pinned: searchParams().has('game'),
  playing: false,
  message: null,
});

const errorMessage = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

// Replay de la page Classement. L'état vit aussi dans une ref : les minuteries et les réponses en retard
// lisent toujours le dernier.
const useReplay = (): { replay: TReplay; actions: TReplayActions } => {
  const [replay, setReplay] = useState<TReplay>(initialReplay);
  const ref = useRef(replay);
  const timer = useRef<ReturnType<typeof setInterval>>(undefined);
  // Dernier replay demandé : une réponse plus ancienne arrivée en retard est ignorée.
  const seq = useRef(0);

  const actions = useMemo(() => {
    const commit = (patch: Partial<TReplay>): void => {
      ref.current = { ...ref.current, ...patch };
      setReplay(ref.current);
    };

    const stopPlayback = (): void => {
      clearInterval(timer.current);
      timer.current = undefined;
      if (ref.current.playing) {
        commit({ playing: false });
      }
    };

    const goTo = (index: number): void => {
      const { id, steps } = ref.current;
      if (id === null || id === '') {
        return;
      }
      commit({ index: Math.max(0, Math.min(steps.length - 1, index)) });
    };

    return {
      // Un échec (API injoignable, partie pas encore en base) est réessayé au rafraîchissement suivant.
      async showGame(row: Pick<TGameRow, 'id' | 'at'>, pinned?: boolean) {
        const isPinned = pinned ?? ref.current.pinned;
        seq.current += 1;
        const { current } = seq;
        let game: TGame;
        try {
          game = await getGame(row);
        } catch (error) {
          if (current === seq.current) {
            commit({ message: `Partie pas encore disponible (${errorMessage(error)}).` });
          }
          return;
        }
        if (current !== seq.current) {
          return;
        }
        stopPlayback();
        const steps = positions(game);
        const moveMs = game.move_ms?.length === game.moves.length ? game.move_ms : [];
        setParam('game', isPinned ? game.id : null);
        commit({
          id: game.id,
          game,
          live: null,
          steps,
          moveMs,
          index: steps.length - 1,
          pinned: isPinned,
          message: null,
        });
      },

      showLive(game: TLiveGame) {
        if (ref.current.live === game) {
          return;
        }
        const { id, index, steps: before } = ref.current;
        const following = id === null || id === '' || index === before.length - 1;
        const steps = positions(game);
        commit({
          id: game.id,
          game: null,
          live: game,
          steps,
          moveMs: game.move_ms,
          index: following ? steps.length - 1 : index,
          message: null,
        });
      },

      // Parcourir une partie la fige : le replay ne saute plus vers la dernière jouée.
      browse(index: number) {
        commit({ pinned: true });
        setParam('game', ref.current.id);
        goTo(index);
      },

      togglePlayback() {
        if (ref.current.id === null || ref.current.id === '') {
          return;
        }
        if (timer.current !== undefined) {
          stopPlayback();
          return;
        }
        if (ref.current.index === ref.current.steps.length - 1) {
          goTo(0);
        }
        commit({ pinned: true, playing: true });
        timer.current = setInterval(() => {
          if (ref.current.index >= ref.current.steps.length - 1) {
            stopPlayback();
            return;
          }
          goTo(ref.current.index + 1);
        }, PLAY_MS);
      },

      // Fige la partie affichée et la met dans l'adresse ; faux sans partie.
      pin() {
        if (ref.current.id === null || ref.current.id === '') {
          return false;
        }
        commit({ pinned: true });
        setParam('game', ref.current.id);
        return true;
      },

      unpin() {
        seq.current += 1;
        commit({ pinned: false });
      },

      setMessage(message: string) {
        commit({ message });
      },

      current: () => ref.current,
    };
  }, []);

  useEffect(() => () => clearInterval(timer.current), []);

  return { replay, actions };
};

export { useReplay };
