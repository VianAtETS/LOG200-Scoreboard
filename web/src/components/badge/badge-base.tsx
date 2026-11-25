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

const Badge: FC<Props> = ({ variant = 'default', title, children }) => (
  <span className={cn('badge', badgeVariantsStyles[variant])} title={title}>
    {children}
  </span>
);

export { Badge };
