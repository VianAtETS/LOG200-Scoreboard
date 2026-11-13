import { REFRESH_MS } from '@/dao/constants/api.const';
import { teamUrl, timelineUrl } from '@/dao/scoreboard.repository';
import { tail, tailState, type TTailState } from '@/lib/tail';
import type { TGameRow } from '@/types/games.types';
import { keepPreviousData, useQuery } from '@tanstack/react-query';
import { useRef } from 'react';
import { queryKeys } from './query-keys';

// Parties des équipes choisies, lues dans teams/<équipe>.jsonl puis suivies par morceaux. Les lectures
// passent à la file : deux lectures en parallèle d'un même fichier doubleraient des lignes.
// `use` : une clé par usage, pour que deux appels sur les mêmes équipes ne partagent pas leur cache.
export const useGetTeamRows = (teams: string[], use = 'picked') => {
  const followed = useRef(new Map<string, TTailState<TGameRow>>());
  const following = useRef<Promise<unknown>>(Promise.resolve());
  // Même Map tant que rien n'arrive : la page ne pulse qu'à l'arrivée de parties.
  const last = useRef<Map<string, TGameRow[]> | null>(null);

  return useQuery({
    queryKey: [queryKeys.teamRows, use, teams.join(',')],
    queryFn: () => {
      for (const team of followed.current.keys()) if (!teams.includes(team)) followed.current.delete(team);
      for (const team of teams) if (!followed.current.has(team)) followed.current.set(team, tailState());
      const states = [...followed.current];
      const run = following.current
        .catch(() => {})
        .then(() => Promise.all(states.map(([team, state]) => tail(teamUrl(team), state))))
        .then((added) => {
          const same = last.current?.size === states.length && states.every(([team]) => last.current?.has(team));
          if (same && !added.some(Boolean)) return last.current!;
          last.current = new Map(states.map(([team, state]) => [team, state.rows]));
          return last.current;
        });
      following.current = run;
      return run;
    },
    refetchInterval: REFRESH_MS,
    placeholderData: keepPreviousData,
    // Comparer des milliers de lignes à chaque tour coûterait plus que de redessiner.
    structuralSharing: false,
  });
};

// timeline.jsonl, lu en entier puis suivi par morceaux. Même tableau tant que rien n'arrive.
export const useGetTimeline = () => {
  const state = useRef(tailState<TGameRow>());

  return useQuery({
    queryKey: [queryKeys.timeline],
    queryFn: async () => {
      await tail(timelineUrl(), state.current);
      return state.current.rows;
    },
    refetchInterval: REFRESH_MS,
    structuralSharing: false,
  });
};
