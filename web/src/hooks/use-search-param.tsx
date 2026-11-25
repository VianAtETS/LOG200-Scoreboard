import { searchParams, setParam } from '@/lib/url';
import { useCallback, useState } from 'react';

// Paramètre d'adresse lu au chargement, puis gardé à jour (replaceState) à chaque choix.
// `keepEmpty` : une valeur vide reste dans l'adresse (?session= : toutes les sessions).
const useSearchParam = (
  key: string,
  keepEmpty = false,
): readonly [string | null, (next: string | null) => void] => {
  const [value, setValue] = useState(() => searchParams().get(key));

  const update = useCallback(
    (next: string | null) => {
      setParam(key, next, keepEmpty);
      setValue(next);
    },
    [key, keepEmpty],
  );

  return [value, update] as const;
};

export { useSearchParam };
