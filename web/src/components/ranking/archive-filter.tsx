import { Badge } from '@/components/badge/badge';
import type { TArchiveOption } from '@/lib/archives';
import { useCallback, useState } from 'react';
import type { ChangeEvent, FC } from 'react';

const HASH_PREVIEW_LENGTH = 8;

type Props = {
  options: TArchiveOption[];
  // Clés cochées, celles de ?archives=.
  wanted: string[];
  onChange: (wanted: string[]) => void;
  error?: string | undefined;
};

const ArchiveFilter: FC<Props> = ({ options, wanted, onChange, error }) => {
  const [query, setQuery] = useState('');
  const search = query.trim().toLowerCase();
  const selected = new Set(wanted);
  const shown = options.filter((option) =>
    `${option.label} ${option.build}`.toLowerCase().includes(search),
  );
  const count = options.filter((option) => selected.has(option.key)).length;

  const handleSearch = useCallback((event: ChangeEvent<HTMLInputElement>): void => {
    setQuery(event.target.value);
  }, []);
  const handleClear = useCallback((): void => onChange([]), [onChange]);
  const handleToggle = useCallback(
    (event: ChangeEvent<HTMLInputElement>): void => {
      const { value, checked } = event.target;
      onChange(checked ? [...wanted, value] : wanted.filter((key) => key !== value));
    },
    [onChange, wanted],
  );

  return (
    <details id="archive-filter" className="archive-filter">
      <summary>Versions archivées ({count})</summary>
      <div className="archive-tools">
        <input
          type="search"
          aria-label="Filtrer les archives par équipe, version ou hash"
          placeholder="Équipe, version ou hash…"
          value={query}
          onChange={handleSearch}
        />
        <button type="button" onClick={handleClear}>
          Tout masquer
        </button>
      </div>
      <fieldset>
        <legend>Versions à afficher</legend>
        {shown.map((option) => (
          <label key={option.key} title={`JAR ${option.build}`}>
            <input
              type="checkbox"
              value={option.key}
              checked={selected.has(option.key)}
              onChange={handleToggle}
            />
            {`${option.label} · ${option.build.slice(0, HASH_PREVIEW_LENGTH)}`}
            <Badge variant="archived">Archivé</Badge>
          </label>
        ))}
        {shown.length === 0 && <p>Aucune version archivée ne correspond.</p>}
      </fieldset>
      <p className="archive-help">
        Archives : anciennes versions, sans nouveaux matchs. Elo figé au dernier match de la version
        ; hors classement actuel.
      </p>
      <output className="archive-status">
        {error !== undefined && error !== '' ? `Archives indisponibles : ${error}` : null}
      </output>
    </details>
  );
};

export { ArchiveFilter };
