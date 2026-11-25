import { getGame } from '@/dao/scoreboard.repository';
import { positions, type TStep } from '@/lib/replay';
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

export type TReplayActions = ReturnType<typeof useReplay>['actions'];

// Replay de la page Classement. L'état vit aussi dans une ref : les minuteries et les réponses en retard
// lisent toujours le dernier.
const useReplay = () => {
  const [replay, setReplay] = useState<TReplay>(() => ({
    id: null,
    game: null,
    live: null,
    steps: [],
    moveMs: [],
    index: 0,
    pinned: searchParams().has('game'),
    playing: false,
    message: null,
  }));
  const ref = useRef(replay);
  const timer = useRef<ReturnType<typeof setInterval>>(undefined);
  // Dernier replay demandé : une réponse plus ancienne arrivée en retard est ignorée.
  const seq = useRef(0);

  const actions = useMemo(() => {
    const commit = (patch: Partial<TReplay>) => {
      ref.current = { ...ref.current, ...patch };
      setReplay(ref.current);
    };

    const stopPlayback = () => {
      clearInterval(timer.current);
      timer.current = undefined;
      if (ref.current.playing) commit({ playing: false });
    };

    const goTo = (i: number) => {
      const { id, steps } = ref.current;
      if (!id) return;
      commit({ index: Math.max(0, Math.min(steps.length - 1, i)) });
    };

    return {
      // Un échec (API injoignable, partie pas encore en base) est réessayé au rafraîchissement suivant.
      async showGame(row: Pick<TGameRow, 'id' | 'at'>, pinned = ref.current.pinned) {
        const current = ++seq.current;
        let g: TGame;
        try {
          g = await getGame(row);
        } catch (e) {
          if (current === seq.current)
            commit({ message: `Partie pas encore disponible (${(e as Error).message}).` });
          return;
        }
        if (current !== seq.current) return;
        stopPlayback();
        const steps = positions(g);
        const moveMs = g.move_ms?.length === g.moves.length ? g.move_ms : [];
        setParam('game', pinned ? g.id : null);
        commit({
          id: g.id,
          game: g,
          live: null,
          steps,
          moveMs,
          index: steps.length - 1,
          pinned,
          message: null,
        });
      },

      showLive(g: TLiveGame) {
        if (ref.current.live === g) return;
        const { id, index, steps: before } = ref.current;
        const following = !id || index === before.length - 1;
        const steps = positions(g);
        commit({
          id: g.id,
          game: null,
          live: g,
          steps,
          moveMs: g.move_ms,
          index: following ? steps.length - 1 : index,
          message: null,
        });
      },

      // Parcourir une partie la fige : le replay ne saute plus vers la dernière jouée.
      browse(i: number) {
        commit({ pinned: true });
        setParam('game', ref.current.id);
        goTo(i);
      },

      togglePlayback() {
        if (!ref.current.id) return;
        if (timer.current) return stopPlayback();
        if (ref.current.index === ref.current.steps.length - 1) goTo(0);
        commit({ pinned: true, playing: true });
        timer.current = setInterval(() => {
          if (ref.current.index >= ref.current.steps.length - 1) return stopPlayback();
          goTo(ref.current.index + 1);
        }, PLAY_MS);
      },

      // Fige la partie affichée et la met dans l'adresse ; faux sans partie.
      pin() {
        if (!ref.current.id) return false;
        commit({ pinned: true });
        setParam('game', ref.current.id);
        return true;
      },

      unpin() {
        seq.current++;
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
