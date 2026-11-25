import { useCallback } from 'react';
import type { ChangeEvent } from 'react';

export function useDuelChoices(
  first: string,
  second: string,
  setBuilds: (value: string | null) => void,
): { choose: (event: ChangeEvent<HTMLSelectElement>) => void; showAll: () => void } {
  const choose = useCallback(
    (event: ChangeEvent<HTMLSelectElement>): void => {
      const { name, value } = event.currentTarget;
      setBuilds(name === '0' ? `${value},${second}` : `${first},${value}`);
    },
    [first, second, setBuilds],
  );
  const showAll = useCallback((): void => setBuilds('*,*'), [setBuilds]);
  return { choose, showAll };
}
