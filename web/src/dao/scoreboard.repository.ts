// Lectures du ladder : fichiers publiés dans /data, et API.

import { API, DATA, LATEST_RELEASE, SERVER } from '@/dao/constants/api.const';
import type { TDuelPair, TDuelSide, TGame, TGameRow, TRunningGame } from '@/types/games.types';
import type { TModel, TQueueFile, TSummary } from '@/types/summary.types';
import type { TTournament, TTournamentIndex } from '@/types/tournament.types';

const HTTP_OK = 200;
const HTTP_ACCEPTED = 202;
const POLL_MS = 3000;
const noStore: RequestInit = { cache: 'no-store' };

async function json<T>(url: string, init: RequestInit = noStore): Promise<T> {
  const res = await fetch(url, init);
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }
  // Callers expect summary, tournament index, tournament, game, or duel-pair contracts
  // from the configured ladder. Payloads are not validated at runtime.
  // oxlint-disable-next-line typescript/no-unsafe-return
  return res.json();
}

async function text(url: string, init: RequestInit = noStore): Promise<string> {
  const res = await fetch(url, init);
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }
  return res.text();
}

export const teamUrl = (team: string): string => `${DATA}/teams/${encodeURIComponent(team)}.jsonl`;
export const timelineUrl = (): string => `${DATA}/timeline.jsonl`;

export const getSummary = async (): Promise<TSummary> => json<TSummary>(`${DATA}/summary.json`);

// Absent avec un ladder qui ne publie pas encore live.json : rien n'est affiché.
export async function getRunning(): Promise<TRunningGame[]> {
  const res = await fetch(`${DATA}/live.json`, noStore);
  if (!res.ok) {
    return [];
  }
  // live.json is a ladder snapshot with a games array; it is not validated at runtime.
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion
  const data = (await res.json()) as { games: TRunningGame[] };
  return data.games;
}

// Absent avec un ladder qui ne publie pas encore queue.json : `null`, et le panneau le dit.
export async function getQueue(): Promise<TQueueFile | null> {
  try {
    const res = await fetch(`${DATA}/queue.json`, noStore);
    if (!res.ok) {
      return null;
    }
    // queue.json follows the ladder queue contract, without runtime validation.
    // oxlint-disable-next-line typescript/no-unsafe-return
    return await res.json();
  } catch {
    return null;
  }
}

// `null` sans model.json.
export async function getModel(): Promise<TModel | null> {
  try {
    const res = await fetch(`${DATA}/model.json`, noStore);
    if (!res.ok) {
      return null;
    }
    // model.json follows the ladder model contract, without runtime validation.
    // oxlint-disable-next-line typescript/no-unsafe-return
    return await res.json();
  } catch {
    return null;
  }
}

// Paires complètes entre deux équipes ; un build absent : toutes ses versions.
export async function getDuelPairs([first, second]: TDuelSide[]): Promise<TDuelPair[]> {
  if (first === undefined || second === undefined) {
    throw new Error('Two duel sides are required');
  }
  const query = new URLSearchParams({ a: first.team, b: second.team });
  if (first.build !== undefined && first.build !== '') {
    query.set('build_a', first.build);
  }
  if (second.build !== undefined && second.build !== '') {
    query.set('build_b', second.build);
  }
  return json<TDuelPair[]>(`${API}/stats/pairs?${query}`);
}

// shortcut: ladder d'avant la base (lignes avec `at`), le temps que page et ladder se déploient ; à retirer ensuite.
async function getGameFromJsonl([start, length]: [number, number]): Promise<TGame> {
  const res = await fetch(`${DATA}/games.jsonl`, {
    headers: { Range: `bytes=${start}-${start + length - 1}` },
  });
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }
  const bytes = new Uint8Array(await res.arrayBuffer());
  const line = res.status === HTTP_OK ? bytes.subarray(start, start + length) : bytes;
  // A line of games.jsonl follows the ladder game contract, without runtime validation.
  // oxlint-disable-next-line typescript/no-unsafe-return
  return JSON.parse(new TextDecoder().decode(line));
}

export async function getGame(row: Pick<TGameRow, 'id' | 'at'>): Promise<TGame> {
  const game = row.at
    ? await getGameFromJsonl(row.at)
    : await json<TGame>(`${API}/games/${encodeURIComponent(row.id)}`, {});
  if (game.id !== row.id) {
    throw new Error('partie introuvable');
  }
  return game;
}

export const getTournamentIndex = async (): Promise<TTournamentIndex> => {
  try {
    return await json<TTournamentIndex>(`${DATA}/tournaments/index.json`);
  } catch {
    return { next: null, tournaments: [] };
  }
};

export const getTournament = async (id: string): Promise<TTournament> =>
  json<TTournament>(`${DATA}/tournaments/${encodeURIComponent(id)}.json`);

// Texte brut : la page compare au précédent pour savoir s'il a changé.
export const getTournamentText = async (id: string): Promise<string> =>
  text(`${DATA}/tournaments/${encodeURIComponent(id)}.json`);

export async function getTournamentPreviewText(): Promise<string | null> {
  try {
    const res = await fetch(`${DATA}/tournament-preview.json`, noStore);
    return res.ok ? await res.text() : null;
  } catch {
    return null;
  }
}

export async function getVersion(): Promise<string | null> {
  try {
    const res = await fetch(`${API}/version`);
    if (!res.ok) {
      return null;
    }
    const version = await res.text();
    return `v${version.trim()}`;
  } catch {
    return null;
  }
}

// GitHub limite à 60 appels anonymes par heure et par adresse : un par visite.
export async function getLatestRelease(): Promise<string | undefined> {
  try {
    const cached = sessionStorage.getItem('latest-release');
    if (cached !== null && cached !== '') {
      return cached;
    }
  } catch {
    // Stockage indisponible : on interroge GitHub.
  }
  const response = await fetch(LATEST_RELEASE);
  if (!response.ok) {
    return undefined;
  }
  // GitHub release responses supply tag_name; the payload is not validated at runtime.
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion
  const { tag_name: tag } = (await response.json()) as { tag_name: string };
  try {
    sessionStorage.setItem('latest-release', tag);
  } catch {
    // Pas de cache : un appel par page.
  }
  return tag;
}

// Le formulaire n'apparaît que si le service de soumission répond (absent en hébergement statique).
export async function getHealth(): Promise<boolean> {
  try {
    const response = await fetch(`${API}/health`, noStore);
    return response.ok;
  } catch {
    return false;
  }
}

// 202 : le verdict est à interroger, Cloudflare coupant une requête à 100 s.
export async function submitJar(
  token: string,
  file: File,
): Promise<{ ok: boolean; message: string }> {
  const headers = { Authorization: `Bearer ${token}` };
  // Error responses also carry the message displayed by the form.
  // oxlint-disable-next-line react-doctor/no-fetch-response-used-without-status-check
  let res = await fetch(`${API}/submit?async`, { method: 'PUT', headers, body: file });
  if (res.status === HTTP_ACCEPTED) {
    const location = await res.text();
    const verdict = `${SERVER}${location.trim()}`;
    do {
      // A 202 response requires a delay before the next request, keeping polling sequential.
      // oxlint-disable-next-line eslint/no-await-in-loop, promise/avoid-new
      await new Promise<void>((resolve) => {
        setTimeout(resolve, POLL_MS);
      });
      // oxlint-disable-next-line eslint/no-await-in-loop, react-doctor/no-fetch-response-used-without-status-check
      res = await fetch(verdict, { headers, cache: 'no-store' });
    } while (res.status === HTTP_ACCEPTED);
  }
  const message = await res.text();
  return { ok: res.ok, message: message.trim() };
}
