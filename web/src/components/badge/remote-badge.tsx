import type { FC } from 'react';
import { Badge } from './badge-base';

const REMOTE_TITLE =
  "Joue depuis la machine de l'équipe : code non vérifié, sans limite de coeurs ni de mémoire, latence réseau comptée";
export const RemoteBadge: FC = () => (
  <Badge variant="remote" title={REMOTE_TITLE}>
    distant
  </Badge>
);
