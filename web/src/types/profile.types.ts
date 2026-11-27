// Profileur : docs/api.md, sections Profilage, Profils et template-share.

import type { TColor, TMeasure } from './games.types';

// Profil du jar d'une équipe dans une partie (GET /api/games/{id}/profile, token d'équipe).
export type TGameProfile = {
  game: string;
  color: TColor;
  build: string;
  // Segments de calcul ; deux tours rapides peuvent n'en faire qu'un.
  turns: number;
  moves: number;
  overhead_ms: number;
  tree: {
    samples: number;
    sampled_ms: number;
    think_ms: number;
    // Le profileur a vu moins de la moitié du temps de réflexion.
    doubtful: boolean;
    rec: number;
    methods: Record<string, [number, number]>;
    paths: Record<string, number>;
  };
  // Lignes ARENA que le jar imprime lui-même.
  arena: TMeasure[];
};

// Choix de profilage du build actif (GET, PUT /api/profiling).
export type TProfiling = { build: string; private: boolean; public: boolean };

// Part publiée du gabarit d'un build (GET /api/stats/template-share), en pour cent.
export type TTemplateShare = {
  team: string;
  build: string;
  games: number;
  samples: number;
  methods: Record<string, number>;
  others: number;
};
