import { useState } from 'react';

// Compte les changements de `value` : pas à sa première valeur définie, ni quand `scope` change
// (une autre équipe, un autre tournoi). Le point du direct pulse à chaque nouveau compte.
const usePulse = (value: unknown, scope: unknown = null): number => {
  const [seen, setSeen] = useState({ value, scope, count: 0 });

  if (seen.value !== value || seen.scope !== scope) {
    const changed = seen.scope === scope && seen.value !== undefined && value !== undefined;
    setSeen({ value, scope, count: seen.count + (changed ? 1 : 0) });
  }

  return seen.count;
};

export { usePulse };
