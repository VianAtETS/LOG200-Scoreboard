// Section Profil de la page Analyse : le profil du jar de l'équipe du token dans la partie, ses choix de
// profilage, et la part du gabarit que les équipes de la partie publient. Le token reste dans l'onglet.

import { queryKeys } from '@/dao/hooks/query-keys';
import { useDescriptor } from '@/dao/hooks/use-descriptor';
import { deleteProfiles, getProfile, getProfiling, getTemplateShare, putProfiling } from '@/dao/scoreboard.repository';
import { useTeamToken } from '@/hooks/use-team-token';
import { describe } from '@/lib/analysis';
import { sideLabel } from '@/lib/game';
import { callTree, topMethods, type TCallNode } from '@/lib/profile';
import type { TColor, TGame } from '@/types/games.types';
import type { TGameProfile, TProfiling } from '@/types/profile.types';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState, type CSSProperties, type FC, type SubmitEvent } from 'react';

const BAR: Record<TColor, string> = { red: 'var(--color-pawn-red)', black: 'var(--color-text)' };
const number = (v: number, digits = 0) => v.toLocaleString('fr-CA', { maximumFractionDigits: digits });
const percent = (n: number, of: number, digits = 1) => `${number(of ? (100 * n) / of : 0, digits)} %`;

// `Classe.` en petit, `méthode` en gras.
const Method: FC<{ name: string }> = ({ name }) => {
  const dot = name.lastIndexOf('.') + 1;
  return (
    <>
      <small>{name.slice(0, dot)}</small>
      <b>{name.slice(dot)}</b>
    </>
  );
};

// Les branches au-dessus de 10 % sont ouvertes au départ, comme dans isola.
const CallRows: FC<{ nodes: TCallNode[]; hidden: number; depth: number; profile: TGameProfile }> = ({ nodes, hidden, depth, profile }) => {
  const { samples, sampled_ms } = profile.tree;
  const indent = { '--d': depth } as CSSProperties;
  const row = (n: TCallNode) => (
    <>
      <span className="fn" title={n.name}>
        <Method name={n.name} />
      </span>
      <span>{percent(n.self, samples)}</span>
      <span className="pbar">
        <i style={{ width: `${(100 * n.total) / samples}%`, background: `color-mix(in srgb, ${BAR[profile.color]} 35%, transparent)` }}>
          <b style={{ width: `${n.total ? (100 * n.self) / n.total : 0}%`, background: BAR[profile.color] }} />
        </i>
      </span>
      <span>{percent(n.total, samples, 0)}</span>
      <span>{profile.moves ? number((n.total / samples) * (sampled_ms / profile.moves)) : '–'}</span>
    </>
  );
  return (
    <>
      {nodes.map((n) =>
        n.kids.length ? (
          <details key={n.path} open={n.total >= samples * 0.1}>
            <summary className="prow" style={indent}>
              {row(n)}
            </summary>
            <CallRows nodes={n.kids} hidden={n.hidden} depth={depth + 1} profile={profile} />
          </details>
        ) : (
          <div key={n.path} className="prow leaf" style={indent}>
            {row(n)}
          </div>
        )
      )}
      {hidden > 0 && (
        <div className="prow more" style={indent}>
          <span className="fn">
            {hidden} {hidden > 1 ? 'autres branches' : 'autre branche'} sous 0,5 %
          </span>
        </div>
      )}
    </>
  );
};

const ProfileView: FC<{ profile: TGameProfile }> = ({ profile }) => {
  const [flat, setFlat] = useState(false);
  const descriptor = useDescriptor();
  const { tree, arena } = profile;
  const root = callTree(tree.paths, tree.samples);
  const depth = describe(arena.flatMap((m) => (m.depth === undefined ? [] : [m.depth])));
  return (
    <>
      <ul className="profile-facts">
        <li>
          Côté {sideLabel(descriptor, profile.color).toLowerCase()}, build <code>{profile.build}</code> : {profile.moves} coups, {profile.turns} segments de calcul.
        </li>
        <li>
          {number(tree.samples)} échantillons, {number(tree.sampled_ms / 1000, 1)} s de calcul vus sur {number(tree.think_ms / 1000, 1)} s de réflexion.
        </li>
        {tree.doubtful && (
          <li className="profile-doubtful" role="note">
            Détection douteuse : le profileur n'a vu que {percent(tree.sampled_ms, tree.think_ms, 0)} du temps de réflexion. Un thread qui lit la socket à part ou des
            threads virtuels lui échappent.
          </li>
        )}
        <li>
          Pile la plus profonde : {tree.rec} appels récursifs. Coût de l'échantillonnage : {number(profile.overhead_ms)} ms, {percent(profile.overhead_ms, tree.think_ms)}{' '}
          du temps de réflexion.
        </li>
        {depth && (
          <li>
            Lignes ARENA de votre jar : profondeur moyenne {number(depth.mean, 1)}, maximale {depth.max}, sur {depth.n} coups.
          </li>
        )}
      </ul>
      {tree.samples > 0 && (
        <>
          <div className="elo-window" role="radiogroup" aria-label="Vue du profil">
            {[
              { value: false, label: 'Arbre' },
              { value: true, label: 'Liste' },
            ].map((o) => (
              <label key={o.label}>
                <input type="radio" name="profile-view" checked={flat === o.value} onChange={() => setFlat(o.value)} />
                <span>{o.label}</span>
              </label>
            ))}
          </div>
          <div className="ptree" role="table" aria-label="Temps par méthode">
            <div className="prow head" role="row">
              <span className="fn" title="Classe.méthode">
                Méthode
              </span>
              <span title="Échantillons où elle est en haut de la pile">Propre</span>
              <span aria-hidden="true" />
              <span title="Échantillons où elle est dans la pile">Total</span>
              <span title="Temps total moyen par coup">ms/coup</span>
            </div>
            {flat ? (
              <CallRows nodes={topMethods(tree.methods)} hidden={0} depth={0} profile={profile} />
            ) : (
              <CallRows nodes={root.kids} hidden={root.hidden} depth={0} profile={profile} />
            )}
          </div>
          <p className="analyse-note">
            {flat
              ? 'Les 20 méthodes au plus de temps propre.'
              : 'Récursion repliée par le profileur ; la barre claire compte les appels, la foncée le temps propre.'}
          </p>
        </>
      )}
    </>
  );
};

// Profiler le jar actif, publier sa part du gabarit, effacer les profils.
const Settings: FC<{ token: string }> = ({ token }) => {
  const client = useQueryClient();
  const key = [queryKeys.profiling, token];
  const { data: state, error } = useQuery({ queryKey: key, queryFn: () => getProfiling(token), retry: false });
  const change = useMutation({
    mutationFn: (choice: Omit<TProfiling, 'build'>) => putProfiling(token, choice),
    onSuccess: (next) => {
      client.setQueryData(key, next);
      client.invalidateQueries({ queryKey: [queryKeys.templateShare] });
    },
  });
  const erase = useMutation({
    mutationFn: () => deleteProfiles(token),
    onSuccess: () => {
      client.invalidateQueries({ queryKey: [queryKeys.profile] });
      client.invalidateQueries({ queryKey: [queryKeys.templateShare] });
    },
  });
  const failure = error ?? change.error ?? erase.error;

  if (state === null) return <p className="analyse-note">Aucun jar actif : soumettez un jar pour le profiler.</p>;
  return (
    <div className="profile-settings">
      {state && (
        <fieldset disabled={change.isPending}>
          <legend>
            Build actif{' '}<code>{state.build}</code>
          </legend>
          <label>
            <input type="checkbox" checked={state.private} onChange={(e) => change.mutate({ private: e.target.checked, public: false })} /> Profiler ses parties du
            ladder (+5 % de temps par coup, jamais en tournoi)
          </label>
          <label>
            <input type="checkbox" checked={state.public} disabled={!state.private} onChange={(e) => change.mutate({ private: true, public: e.target.checked })} />{' '}
            Publier la part du gabarit : temps dans le code de départ du cours, sans arbre ni profondeur
          </label>
        </fieldset>
      )}
      <button
        type="button"
        disabled={erase.isPending}
        onClick={() => window.confirm("Effacer tous les profils de l'équipe, part publiée comprise ?") && erase.mutate()}
      >
        Effacer tous nos profils
      </button>
      {erase.data && <span role="status"> {erase.data.deleted} profils effacés.</span>}
      {failure && <p className="submit-status" data-state="error" role="alert">{failure.message}</p>}
    </div>
  );
};

// Part publiée de chaque côté de la partie ; rien si aucune équipe ne publie.
const TemplateShares: FC<{ game: TGame }> = ({ game }) => {
  const sides = (['red', 'black'] as const).flatMap((c) => (game[c].build ? [{ team: game[c].team, build: game[c].build! }] : []));
  const shares = useQuery({
    queryKey: [queryKeys.templateShare, game.id],
    queryFn: () => Promise.all(sides.map((s) => getTemplateShare(s.team, s.build))),
    staleTime: 60_000,
  });
  const published = (shares.data ?? []).flatMap((s) => (s ? [s] : []));
  if (!published.length) return null;
  return (
    <>
      <h3>Part du gabarit publiée</h3>
      {published.map((s) => (
        <p key={s.team} className="analyse-note">
          <b>{s.team}</b> sur {s.games} parties profilées de ce build :{' '}
          {Object.entries(s.methods)
            .filter(([, v]) => v > 0)
            .map(([m, v]) => `${m} ${number(v, 1)} %`)
            .join(', ') || 'aucune méthode du gabarit'}
          , autres {number(s.others, 1)} %. Chiffres déclarés par le jar.
        </p>
      ))}
    </>
  );
};

const ProfileSection: FC<{ game: TGame }> = ({ game }) => {
  const [token, setToken] = useTeamToken();
  const [draft, setDraft] = useState('');
  const profile = useQuery({
    queryKey: [queryKeys.profile, game.id, token],
    queryFn: () => getProfile(game.id, token),
    enabled: Boolean(token),
    retry: false,
  });

  const handleSubmit = (e: SubmitEvent) => {
    e.preventDefault();
    setToken(draft.trim());
    setDraft('');
  };

  return (
    <section className="section analyse-profile" aria-labelledby="profile-title">
      <h2 id="profile-title">Profil</h2>
      {token ? (
        <>
          <p className="analyse-note">
            {profile.isPending
              ? 'Chargement du profil…'
              : profile.error
                ? `Profil indisponible : ${profile.error.message}`
                : profile.data === null
                  ? "Aucun profil de votre équipe dans cette partie : elle ne l'a pas jouée, ou son jar n'était pas profilé."
                  : null}{' '}
            <button type="button" className="link" onClick={() => setToken('')}>
              Oublier le token
            </button>
          </p>
          {profile.data && <ProfileView profile={profile.data} />}
          {!profile.error && <Settings token={token} />}
        </>
      ) : (
        <form className="profile-form" onSubmit={handleSubmit}>
          <label className="field" htmlFor="profile-token">
            <span>Token de l'équipe : votre profil dans cette partie, s'il existe. Il reste dans cet onglet.</span>
            <input
              id="profile-token"
              type="password"
              autoComplete="off"
              spellCheck={false}
              required
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
            />
          </label>
          <button className="primary" type="submit">
            Voir mon profil
          </button>
        </form>
      )}
      <TemplateShares game={game} />
    </section>
  );
};

export { ProfileSection };
