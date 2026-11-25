// File d'attente du ladder (queue.json), dessinée comme un plateau : un arbre binaire de recherche
// équilibré dont l'ordre infixe est celui de la file. La prochaine équipe libre est la case rouge.
// À partir de LIST_FROM équipes, l'arbre serait trop large : les cases se rangent en liste.
// Le dessin est dans components/queue/ ; ici, la géométrie et les textes.

import type { TColor } from '../types/games.types';
import type { TQueueTeam } from '../types/summary.types';

export const LIST_FROM = 60;
export const COLUMN = 72;
export const ROW = 70;
export const TILE = { w: 64, h: 40 };
export const TOP = 10;
export const BELOW = 54;
const LABEL = 9;
const BEND = 7;
export const STEP_MS = 90;

export type TQueueNode = TQueueTeam & { depth: number; parent: number | null; path: boolean };

export type TPoint = { x: number; y: number };

// `busy` : couleur du pion d'une équipe en partie ; `called` : équipes annoncées.
export type TQueueMarks = { busy: Map<string, TColor>; called: Set<string> };

// Nœuds dans l'ordre de la file : `parent` est un indice, `path` marque le chemin de la racine à `next`.
export function layout(teams: TQueueTeam[], next: number) {
  const nodes: TQueueNode[] = teams.map((team) => ({
    ...team,
    depth: 0,
    parent: null,
    path: false,
  }));
  const place = (lo: number, hi: number, depth: number, parent: number | null) => {
    if (lo >= hi) {
      return;
    }
    const mid = (lo + hi - 1) >> 1;
    Object.assign(nodes[mid], { depth, parent, path: lo <= next && next < hi });
    place(lo, mid, depth + 1, mid);
    place(mid + 1, hi, depth + 1, mid);
  };
  place(0, nodes.length, 0, null);
  return nodes;
}

export const signed = (n: number) => `${n < 0 ? '−' : '+'}${Math.abs(n)}`;
export const short = (id: string) => (id.length > LABEL ? `${id.slice(0, LABEL - 1)}…` : id);

// Coude arrondi de `a` vers `b`, depuis un point déjà placé à l'abscisse de `a`, jusqu'à l'ordonnée `to`.
export function bend(a: TPoint, b: TPoint, to: number) {
  const mid = (a.y + TILE.h / 2 + b.y - TILE.h / 2) / 2;
  const s = Math.sign(b.x - a.x);
  return `V${mid - BEND}Q${a.x} ${mid} ${a.x + s * BEND} ${mid}H${b.x - s * BEND}Q${b.x} ${mid} ${b.x} ${mid + BEND}V${to}`;
}

// Ce que la case montre.
export function kinds(team: TQueueTeam, { busy, called }: TQueueMarks, next: boolean) {
  return [
    team.bot ? 'bot' : 'team',
    next && 'next',
    busy.has(team.id) && 'busy',
    called.has(team.id) && 'called',
  ];
}

export function describe(team: TQueueTeam, { busy, called }: TQueueMarks) {
  const pawn = busy.get(team.id);
  const where = pawn ? `, en partie avec les ${pawn === 'red' ? 'rouges' : 'noirs'}` : '';
  return `${team.id} : ${signed(team.ahead)}${where}${called.has(team.id) ? ', annoncée' : ''}`;
}

// Temps restant avant `at`, pour l'annonce : « dans 42 s », « dans 1 min 05 » ou « maintenant ».
export function countdown(at: string, now: number) {
  const s = Math.ceil((Date.parse(at) - now) / 1000);
  if (!(s > 0)) {
    return 'maintenant';
  }
  return s < 60
    ? `dans ${s} s`
    : `dans ${Math.floor(s / 60)} min ${String(s % 60).padStart(2, '0')}`;
}
