import { LiveIcon } from '@/components/icons/icons';
import { pageUrl } from '@/lib/url';
import type { FC } from 'react';

type Props = {
  id: string;
  title: string;
};

// Lien vers une partie en cours, suivie en direct par le replay : une icône d'antenne.
const LiveLink: FC<Props> = ({ id, title }) => (
  <a className="live-link" href={pageUrl('./', { game: id })} title={title} aria-label={title}>
    <LiveIcon />
  </a>
);

export { LiveLink };
