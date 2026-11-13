import { cn } from '@/lib/utils';
import type { FC, PropsWithChildren } from 'react';

const badgeVariantsStyles = {
  default: '',
  remote: 'remote',
  anchor: 'anchor',
  archived: 'archived',
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

export { Badge, RemoteBadge };
