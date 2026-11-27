// Arbre d'appels d'un profil (GET /api/games/{id}/profile), d'après treeRows et flatRows de l'arène
// isola de Justin Pagès (gitlab.com/justin-pages/isola).

export type TCallNode = {
  // `Classe.méthode`.
  name: string;
  // Chemin depuis la racine, `A.f>B.g` : identifie le nœud.
  path: string;
  // Échantillons où elle est en haut de la pile, et où elle est dans la pile.
  self: number;
  total: number;
  kids: TCallNode[];
  // Branches filles regroupées, sous 0,5 % des échantillons.
  hidden: number;
};

// La racine (sans nom) de l'arbre des chemins `A.f>B.g` → échantillons dont c'est la pile.
export function callTree(paths: Record<string, number>, samples: number): TCallNode {
  type TBuild = { self: number; total: number; kids: Map<string, TBuild> };
  const root: TBuild = { self: 0, total: 0, kids: new Map() };
  for (const [path, n] of Object.entries(paths)) {
    let node = root;
    for (const name of path.split('>')) {
      if (!node.kids.has(name)) node.kids.set(name, { self: 0, total: 0, kids: new Map() });
      node = node.kids.get(name)!;
      node.total += n;
    }
    node.self += n;
  }
  const small = samples * 0.005;
  const done = (name: string, path: string, node: TBuild): TCallNode => {
    const kids = [...node.kids].sort((x, y) => y[1].total - x[1].total);
    const big = kids.filter(([, kid]) => kid.total >= small);
    return {
      name,
      path,
      self: node.self,
      total: node.total,
      kids: big.map(([kid, k]) => done(kid, path ? `${path}>${kid}` : kid, k)),
      hidden: kids.length - big.length,
    };
  };
  return done('', '', root);
}

// Les 20 méthodes au plus de temps propre, en feuilles.
export function topMethods(methods: Record<string, [number, number]>): TCallNode[] {
  return Object.entries(methods)
    .sort((x, y) => y[1][0] - x[1][0])
    .slice(0, 20)
    .map(([name, [self, total]]) => ({ name, path: name, self, total, kids: [], hidden: 0 }));
}
