import { cn } from '@/lib/utils';
import type { FC, PropsWithChildren } from 'react';

const badgeVariantsStyles = {
  default: '',
  remote: 'remote',
  anchor: 'anchor',
  archived: 'archived',
  node: 'node',
};

type TBadgeVariant = keyof typeof badgeVariantsStyles;

type Props = PropsWithChildren & {
  variant?: TBadgeVariant;
  title?: string;
};

const Badge: FC<Props> = ({ variant = 'default', title, children }) => {
  return (
    <span className={cn('badge', badgeVariantsStyles[variant])} title={title}>
      {children}
    </span>
  );
};

const REMOTE_TITLE =
  "Joue depuis la machine de l'équipe : code non vérifié, sans limite de coeurs ni de mémoire, latence réseau comptée";

const RemoteBadge: FC = () => (
  <Badge variant="remote" title={REMOTE_TITLE}>
    distant
  </Badge>
);

const NODE_TITLE =
  'Jouée sur un nœud de calcul, plus lent que le serveur principal : mêmes règles et même temps par coup, mais des temps de réflexion plus longs, laissés hors des statistiques de temps';

const NodeBadge: FC = () => (
  <Badge variant="node" title={NODE_TITLE}>
    nœud
  </Badge>
);

export { Badge, NodeBadge, RemoteBadge };
