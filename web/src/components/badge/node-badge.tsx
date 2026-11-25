import type { FC } from 'react';
import { Badge } from './badge-base';

const NODE_TITLE =
  'Jouée sur un nœud de calcul, plus lent que le serveur principal : mêmes règles et même temps par coup, mais des temps de réflexion plus longs, laissés hors des statistiques de temps';
export const NodeBadge: FC = () => (
  <Badge variant="node" title={NODE_TITLE}>
    nœud
  </Badge>
);
