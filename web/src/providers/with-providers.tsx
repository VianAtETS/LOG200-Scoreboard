import { createElement } from 'react';
import type { ComponentType, FC } from 'react';
import { QueryProvider } from './query-provider';

// Chaque page Astro monte une île React avec ses propres providers.
export function withProviders(Page: ComponentType): FC {
  const PageWithProviders: FC = () => <QueryProvider>{createElement(Page)}</QueryProvider>;
  PageWithProviders.displayName = `WithProviders(${Page.displayName ?? Page.name})`;
  return PageWithProviders;
}
