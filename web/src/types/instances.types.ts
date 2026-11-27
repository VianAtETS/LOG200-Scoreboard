// instances.json, à la racine d'un serveur à plusieurs jeux (deploy/Caddyfile.multi) : une entrée par
// ladder, servi sous /<id>/.
export type TInstance = {
  id: string;
  // Précision affichée sous le nom du jeu : la session, le groupe.
  label?: string;
  // Session terminée : lecture seule (`archived` dans son ladder.toml).
  archived?: boolean;
};

export type TInstances = { v: number; instances: TInstance[] };
