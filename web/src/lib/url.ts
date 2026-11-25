// Les choix de la page vivent dans l'adresse : elle se partage telle quelle.

export const searchParams = () => new URLSearchParams(location.search);

export function setParam(key: string, value: string | null | undefined, keepEmpty = false) {
  const url = new URL(location.href);
  if (value || (keepEmpty && value === '')) {
    url.searchParams.set(key, value);
  } else {
    url.searchParams.delete(key);
  }
  // Virgules lisibles dans ?teams=.
  url.search = url.searchParams.toString().replaceAll('%2C', ',');
  history.replaceState(null, '', url);
}

// Adresse d'une autre page, avec les choix communs aux pages et les paramètres de `extra`.
// Une valeur vide reste dans l'adresse (aucune équipe, toutes les sessions).
export function pageUrl(href: string, extra: Record<string, string> = {}) {
  const url = new URL(href, location.href);
  const here = searchParams();
  for (const key of [
    'data',
    'session',
    'window',
    'teams',
    'match',
    'verified',
    'metric',
    'elo',
    'record',
    'pace',
    'sort',
    'order',
    'side',
  ]) {
    const value = here.get(key);
    if (value !== null) {
      url.searchParams.set(key, value);
    }
  }
  for (const [key, value] of Object.entries(extra)) {
    url.searchParams.set(key, value);
  }
  url.search = url.searchParams.toString().replaceAll('%2C', ',');
  return url.href;
}
