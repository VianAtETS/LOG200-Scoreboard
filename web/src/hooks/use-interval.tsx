import { useEffect, useRef } from 'react';

// `callback` toutes les `ms`, `null` pour arrêter.
const useInterval = (callback: () => void, ms: number | null): void => {
  const saved = useRef(callback);

  useEffect(() => {
    saved.current = callback;
  });

  useEffect(() => {
    if (ms === null) {
      return () => {
        // No timer was started.
      };
    }
    const timer = setInterval(() => saved.current(), ms);
    return () => clearInterval(timer);
  }, [ms]);
};

export { useInterval };
