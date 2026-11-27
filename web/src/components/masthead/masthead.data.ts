export const pages = [
  { key: 'classement', label: 'Classement', href: './' },
  { key: 'graphe', label: 'Graphe', href: 'graphe.html' },
  { key: 'analyse', label: 'Analyse', href: 'analyse.html' },
  { key: 'tournoi', label: 'Tournoi', href: 'tournoi.html' },
  { key: 'direct', label: 'Direct', href: 'direct.html' },
  { key: 'api', label: 'API', href: 'api.html' },
] as const;

export type TPageKey = (typeof pages)[number]['key'];
