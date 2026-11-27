// Accueil d'un serveur à plusieurs jeux (deploy/Caddyfile.multi) : un lien par instance, avec son jeu
// et où en est sa session. Servi sous /web/accueil.html, à côté de instances.json.

import { Masthead } from '@/components/masthead/masthead';
import { SERVER } from '@/dao/constants/api.const';
import { useInstanceCard, useInstances } from '@/dao/hooks/use-instances';
import { fullFormat } from '@/lib/format';
import { instanceUrls } from '@/lib/instances';
import { withProviders } from '@/providers/with-providers';
import type { TInstance } from '@/types/instances.types';
import type { FC } from 'react';

const InstanceCard: FC<{ instance: TInstance }> = ({ instance }) => {
  const urls = instanceUrls(SERVER, instance.id);
  const { data } = useInstanceCard(urls.data);
  const summary = data?.summary;
  return (
    <li className="instance">
      <a href={urls.page}>
        <h2>
          {data?.descriptor.name ?? instance.id}
          {instance.archived && <span className="badge">terminée</span>}
        </h2>
        {instance.label && <p className="instance-label">{instance.label}</p>}
        <p className="instance-meta">
          {!data
            ? 'Chargement…'
            : !summary
              ? 'Ladder injoignable'
              : `${summary.teams} équipes · ${summary.games} parties` +
                (summary.last ? ` · dernière le ${fullFormat.format(new Date(summary.last))}` : '')}
        </p>
      </a>
    </li>
  );
};

const HomePage = () => {
  const { data: instances, isPending } = useInstances(SERVER);
  return (
    <>
      <Masthead home showVersion={false} />
      <main className="layout">
        <section className="section" aria-labelledby="instances-title">
          <h2 id="instances-title">Jeux</h2>
          {isPending ? (
            <p>Chargement…</p>
          ) : !instances?.length ? (
            <p>
              Ce serveur n'a pas de liste de jeux (<code>instances.json</code>). <a href="./">Ouvrir le ladder</a>.
            </p>
          ) : (
            <ul className="instances">
              {instances.map((instance) => (
                <InstanceCard key={instance.id} instance={instance} />
              ))}
            </ul>
          )}
        </section>
      </main>
    </>
  );
};

export default withProviders(HomePage);
