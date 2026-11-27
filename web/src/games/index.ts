// Rendus de plateau propres à un jeu, par identifiant du descripteur (`game`). Facultatifs : sans
// rendu ici, un jeu est dessiné par la grille générique (components/board/board.tsx), qui suffit à
// tout jeu sur grille décrit par son descripteur. Un rendu propre reçoit la position, ses effets et le
// descripteur, jamais les règles ; s'il plante, la grille le remplace.
//
// Pour en ajouter un : un composant `FC<BoardProps>` dans web/src/games/<jeu>/, enregistré ici.

import type { BoardProps } from '@/components/board/board';
import type { FC } from 'react';

export const renderers: Partial<Record<string, FC<BoardProps>>> = {};
