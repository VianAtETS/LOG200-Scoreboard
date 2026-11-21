import { useCallback, useState } from 'react';

// Partagé par la soumission et le profil ; oublié à la fermeture de l'onglet.
const KEY = 'team-token';

const read = () => {
  try {
    return sessionStorage.getItem(KEY) ?? '';
  } catch {
    return '';
  }
};

export const useTeamToken = () => {
  const [token, setToken] = useState(read);
  const save = useCallback((value: string) => {
    setToken(value);
    try {
      if (value) sessionStorage.setItem(KEY, value);
      else sessionStorage.removeItem(KEY);
    } catch {
      // Stockage indisponible : le token vit le temps de la page.
    }
  }, []);
  return [token, save] as const;
};
