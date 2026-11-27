// Serveur à plusieurs jeux : la liste des instances (instances.json, à sa racine) et leurs adresses.
// Chaque instance sert la même page sous /<id>/web/, ses fichiers sous /<id>/data/, son API sous /<id>/api/.

import type { TInstance } from '../types/instances.types';

// Un id finit dans des chemins : rien d'autre que des minuscules, chiffres, `-` et `_`.
const ID = /^[a-z0-9][a-z0-9_-]{0,47}$/;

// Les instances utilisables d'un instances.json ; `null` s'il est illisible ou d'une autre version.
export function parseInstances(value: unknown): TInstance[] | null {
  const file = value as { v?: unknown; instances?: unknown } | null;
  if (!file || file.v !== 1 || !Array.isArray(file.instances)) return null;
  return file.instances.filter(
    (i): i is TInstance =>
      !!i &&
      typeof i.id === 'string' &&
      ID.test(i.id) &&
      (i.label === undefined || typeof i.label === 'string') &&
      (i.archived === undefined || typeof i.archived === 'boolean')
  );
}

// `root` : la racine du serveur à plusieurs jeux, vue de la page.
export const instanceUrls = (root: string, id: string) => ({
  page: `${root}/${id}/web/`,
  data: `${root}/${id}/data`,
});

// La racine du serveur à plusieurs jeux vue d'une instance : `server` est la racine de l'instance.
export const rootOf = (server: string) => `${server}/..`;
