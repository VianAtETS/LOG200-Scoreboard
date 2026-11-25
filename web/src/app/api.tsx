// En-tête de la page API : la documentation (Swagger UI) est chargée par la page Astro.

import { Masthead } from '@/components/masthead/masthead';
import { withProviders } from '@/providers/with-providers';
import type { JSX } from 'react';

const ApiHeader = (): JSX.Element => <Masthead current="api" showVersion={false} />;

export const ApiPage = withProviders(ApiHeader);
