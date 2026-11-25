// Valeur des listes "Toutes" et "Tout".
export const GLOBAL = '';

// Fenêtres des graphes en ms ; une valeur absente vaut "Tout".
export const WINDOWS: Record<string, number> = { '1h': 36e5, '24h': 864e5, '7j': 6048e5 };

export const WINDOW_OPTIONS = [
  { value: '1h', label: '1 h' },
  { value: '24h', label: '24 h' },
  { value: '7j', label: '7 j' },
  { value: GLOBAL, label: 'Tout' },
];

export const windowOf = (value: string | null) =>
  value !== null && value in WINDOWS ? value : GLOBAL;
