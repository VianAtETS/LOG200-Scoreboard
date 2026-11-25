// En-tête de la page API : la documentation (Swagger UI) est chargée par la page Astro.

import { Masthead } from '@/components/masthead/masthead';
import { withProviders } from '@/providers/with-providers';

const ApiHeader = () => <Masthead current="api" showVersion={false} />;

export default withProviders(ApiHeader);
