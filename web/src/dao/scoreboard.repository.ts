// Lectures du ladder : fichiers publiés dans /data, et API.

import { API, DATA, LATEST_RELEASE, SERVER } from '@/dao/constants/api.const';
import { ISOLATION, usable } from '@/lib/game';
import { parseInstances } from '@/lib/instances';
import type { TDescriptor } from '@/types/game.types';
import type { TDuelPair, TDuelSide, TGame, TGameRow, THeatmap, TRunningGame } from '@/types/games.types';
import type { TGameProfile, TProfiling, TTemplateShare } from '@/types/profile.types';
import type { TModel, TQueueFile, TSummary } from '@/types/summary.types';
import type { TTournament, TTournamentIndex } from '@/types/tournament.types';
import type { TInstance } from '@/types/instances.types';

const noStore: RequestInit = { cache: 'no-store' };

async function json<T>(url: string, init: RequestInit = noStore): Promise<T> {
  const res = await fetch(url, init);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

async function text(url: string, init: RequestInit = noStore) {
  const res = await fetch(url, init);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.text();
}

export const teamUrl = (team: string) => `${DATA}/teams/${encodeURIComponent(team)}.jsonl`;
export const timelineUrl = () => `${DATA}/timeline.jsonl`;

export const getSummary = () => json<TSummary>(`${DATA}/summary.json`);

// Absent avec un ladder qui ne publie pas encore live.json : rien n'est affiché.
export async function getRunning(): Promise<TRunningGame[]> {
  const res = await fetch(`${DATA}/live.json`, noStore);
  return res.ok ? (await res.json()).games : [];
}

// Absent avec un ladder qui ne publie pas encore queue.json : `null`, et le panneau le dit.
export async function getQueue(): Promise<TQueueFile | null> {
  const res = await fetch(`${DATA}/queue.json`, noStore).catch(() => null);
  return res?.ok ? await res.json().catch(() => null) : null;
}

// `null` sans model.json.
export async function getModel(): Promise<TModel | null> {
  try {
    const res = await fetch(`${DATA}/model.json`, noStore);
    return res.ok ? await res.json() : null;
  } catch {
    return null;
  }
}

// Paires complètes entre deux équipes ; un build absent : toutes ses versions.
export function getDuelPairs([a, b]: TDuelSide[]) {
  const query = new URLSearchParams({ a: a.team, b: b.team });
  if (a.build) query.set('build_a', a.build);
  if (b.build) query.set('build_b', b.build);
  return json<TDuelPair[]>(`${API}/stats/pairs?${query}`);
}

// Cartes de chaleur de chaque build de l'équipe, hors tournois.
export const getHeatmaps = (team: string) => json<THeatmap[]>(`${API}/stats/heatmap?${new URLSearchParams({ team })}`);

export async function getGame(row: Pick<TGameRow, 'id' | 'at'>): Promise<TGame> {
  const g = row.at ? await getGameFromJsonl(row.at) : await json<TGame>(`${API}/games/${encodeURIComponent(row.id)}`, {});
  if (g.id !== row.id) throw new Error('partie introuvable');
  return g;
}

// shortcut: ladder d'avant la base (lignes avec `at`), le temps que page et ladder se déploient ; à retirer ensuite.
async function getGameFromJsonl([start, length]: [number, number]): Promise<TGame> {
  const res = await fetch(`${DATA}/games.jsonl`, { headers: { Range: `bytes=${start}-${start + length - 1}` } });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const bytes = new Uint8Array(await res.arrayBuffer());
  const line = res.status === 200 ? bytes.subarray(start, start + length) : bytes;
  return JSON.parse(new TextDecoder().decode(line));
}

export const getTournamentIndex = () =>
  json<TTournamentIndex>(`${DATA}/tournaments/index.json`).catch((): TTournamentIndex => ({ next: null, tournaments: [] }));

export const getTournament = (id: string) => json<TTournament>(`${DATA}/tournaments/${encodeURIComponent(id)}.json`);

// Texte brut : la page compare au précédent pour savoir s'il a changé.
export const getTournamentText = (id: string) => text(`${DATA}/tournaments/${encodeURIComponent(id)}.json`);

export async function getTournamentPreviewText() {
  return fetch(`${DATA}/tournament-preview.json`, noStore)
    .then((res) => (res.ok ? res.text() : null))
    .catch(() => null);
}

export async function getVersion() {
  const res = await fetch(`${API}/version`).catch(() => null);
  return res?.ok ? `v${(await res.text()).trim()}` : null;
}

// GitHub limite à 60 appels anonymes par heure et par adresse : un par visite.
export async function getLatestRelease(): Promise<string | undefined> {
  try {
    const cached = sessionStorage.getItem('latest-release');
    if (cached) return cached;
  } catch {
    // Stockage indisponible : on interroge GitHub.
  }
  const response = await fetch(LATEST_RELEASE);
  if (!response.ok) return undefined;
  const { tag_name: tag } = await response.json();
  try {
    sessionStorage.setItem('latest-release', tag);
  } catch {
    // Pas de cache : un appel par page.
  }
  return tag;
}

// Le formulaire n'apparaît que si le service de soumission répond (absent en hébergement statique).
export async function getHealth() {
  try {
    return (await fetch(`${API}/health`, noStore)).ok;
  } catch {
    return false;
  }
}

// 202 : le verdict est à interroger, Cloudflare coupant une requête à 100 s.
export async function submitJar(token: string, file: File) {
  const headers = { Authorization: `Bearer ${token}` };
  let res = await fetch(`${API}/submit?async`, { method: 'PUT', headers, body: file });
  if (res.status === 202) {
    const verdict = `${SERVER}${(await res.text()).trim()}`;
    do {
      await new Promise((resolve) => setTimeout(resolve, 3000));
      res = await fetch(verdict, { headers, cache: 'no-store' });
    } while (res.status === 202);
  }
  return { ok: res.ok, message: (await res.text()).trim() };
}

// Route d'équipe : `null` sur 404 ; un refus lève avec le message du ladder.
async function asTeam<T>(path: string, token: string, init: RequestInit = {}): Promise<T | null> {
  const res = await fetch(`${API}${path}`, { ...init, cache: 'no-store', headers: { Authorization: `Bearer ${token}` } });
  if (res.status === 404) return null;
  if (!res.ok) throw new Error((await res.text()).trim() || `HTTP ${res.status}`);
  return res.json();
}

export const getProfile = (game: string, token: string) =>
  asTeam<TGameProfile>(`/games/${encodeURIComponent(game)}/profile`, token);

// `null` : aucun jar actif.
export const getProfiling = (token: string) => asTeam<TProfiling>('/profiling', token);

export const putProfiling = (token: string, choice: Omit<TProfiling, 'build'>) =>
  asTeam<TProfiling>('/profiling', token, { method: 'PUT', body: JSON.stringify(choice) });

export const deleteProfiles = (token: string) => asTeam<{ deleted: number }>('/profiles', token, { method: 'DELETE' });

// `null` : part non publiée.
export async function getTemplateShare(team: string, build: string) {
  const res = await fetch(`${API}/stats/template-share?${new URLSearchParams({ team, build })}`);
  return res.ok ? (res.json() as Promise<TTemplateShare>) : null;
}

// Le jeu de l'instance (data/game.json). Absent ou incohérent (ladder d'avant la plateforme
// multi-jeux) : Isolation.
export async function getDescriptor(data = DATA): Promise<TDescriptor> {
  const res = await fetch(`${data}/game.json`, noStore).catch(() => null);
  const value = res?.ok ? await res.json().catch(() => null) : null;
  return usable(value) ? value : ISOLATION;
}

// Les instances d'un serveur à plusieurs jeux (instances.json à sa racine `root`). `null` sans lui : un
// serveur d'une seule instance.
export async function getInstances(root: string): Promise<TInstance[] | null> {
  const res = await fetch(`${root}/instances.json`, noStore).catch(() => null);
  return res?.ok ? parseInstances(await res.json().catch(() => null)) : null;
}

// Le résumé d'une instance pour l'accueil : son jeu, ses équipes classées, ses parties. `summary` vaut
// `null` si l'instance ne répond pas (arrêtée, pas encore démarrée).
export async function getInstanceCard(data: string) {
  const [descriptor, summary] = await Promise.all([
    getDescriptor(data),
    json<TSummary>(`${data}/summary.json`).catch(() => null),
  ]);
  const all = summary?.scopes[''];
  return {
    descriptor,
    summary: all ? { teams: all.ratings.filter((r) => !r.anchor).length, games: all.games, last: all.recent.at(-1)?.ts } : null,
    sessions: summary?.sessions ?? [],
  };
}
