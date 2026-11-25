import { useCallback, useLayoutEffect, useState } from 'react';
import type { RefObject } from 'react';

// Ombres .less / .more d'une zone qui défile : du contenu caché au-dessus ou en dessous.
const useScrollEdges = (
  ref: RefObject<HTMLElement | null>,
): { edges: { less: boolean; more: boolean }; onScroll: () => void } => {
  const [edges, setEdges] = useState({ less: false, more: false });

  const measure = useCallback(() => {
    const node = ref.current;
    if (!node) {
      return;
    }
    const less = node.scrollTop > 2;
    const more = node.scrollTop + node.clientHeight < node.scrollHeight - 2;
    setEdges((prev) => (prev.less === less && prev.more === more ? prev : { less, more }));
  }, [ref]);

  // Après chaque rendu : le contenu a pu changer de hauteur.
  useLayoutEffect(measure);

  useLayoutEffect(() => {
    const node = ref.current;
    const observer = node ? new ResizeObserver(measure) : undefined;
    if (node) {
      observer?.observe(node);
    }
    return () => observer?.disconnect();
  }, [ref, measure]);

  return { edges, onScroll: measure };
};

export { useScrollEdges };
