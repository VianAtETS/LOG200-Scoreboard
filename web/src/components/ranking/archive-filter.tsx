import { Badge } from '@/components/badge/badge';
import type { TArchiveOption } from '@/lib/archives';
import { useState, type FC } from 'react';

type Props = {
  options: TArchiveOption[];
  // Clés cochées, celles de ?archives=.
  wanted: string[];
  onChange: (wanted: string[]) => void;
  error?: string;
};

const ArchiveFilter: FC<Props> = ({ options, wanted, onChange, error }) => {
  const [query, setQuery] = useState('');
  const q = query.trim().toLowerCase();
  const shown = options.filter((o) => `${o.label} ${o.build}`.toLowerCase().includes(q));
  const count = options.filter((o) => wanted.includes(o.key)).length;

  const toggle = (key: string, checked: boolean) =>
    onChange(checked ? [...wanted, key] : wanted.filter((k) => k !== key));

  return (
    <details id="archive-filter" className="archive-filter">
      <summary>Versions archivées ({count})</summary>
      <div className="archive-tools">
        <input
          type="search"
          aria-label="Filtrer les archives par équipe, version ou hash"
          placeholder="Équipe, version ou hash…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button type="button" onClick={() => onChange([])}>
          Tout masquer
        </button>
      </div>
      <fieldset>
        <legend>Versions à afficher</legend>
        {shown.map((o) => (
          <label key={o.key} title={`JAR ${o.build}`}>
            <input
              type="checkbox"
              value={o.key}
              checked={wanted.includes(o.key)}
              onChange={(e) => toggle(o.key, e.target.checked)}
            />
            {`${o.label} · ${o.build.slice(0, 8)}`}
            <Badge variant="archived">Archivé</Badge>
          </label>
        ))}
        {!shown.length && <p>Aucune version archivée ne correspond.</p>}
      </fieldset>
      <p className="archive-help">
        Archives : anciennes versions, sans nouveaux matchs. Elo figé au dernier match de la version ; hors classement
        actuel.
      </p>
      <p className="archive-status" role="status">
        {error && `Archives indisponibles : ${error}`}
      </p>
    </details>
  );
};

export { ArchiveFilter };
