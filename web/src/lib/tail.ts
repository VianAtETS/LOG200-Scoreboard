import { parseJsonl } from './elo.ts';

export type TTailState<T> = { offset: number | null; rows: T[] };

export const tailState = <T>(): TTailState<T> => ({ offset: null, rows: [] });

// JSONL en ajout seul : lu en entier, puis seulement la suite (Range). Vrai si des lignes sont arrivées.
export async function tail<T extends { v: number }>(
  url: string,
  state: TTailState<T>,
): Promise<boolean> {
  const headers: HeadersInit = state.offset ? { Range: `bytes=${state.offset}-` } : {};
  const res = await fetch(url, { cache: 'no-store', headers });
  if (res.status === 416) {
    // Rien après offset, sauf si le fichier a rétréci (réécrit) : on relit tout.
    if (Number(res.headers.get('Content-Range')?.split('/')[1]) < state.offset!) {
      state.offset = 0;
      return tail(url, state);
    }
    return false;
  }
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const bytes = new Uint8Array(await res.arrayBuffer());
  // Une dernière ligne en cours d'écriture est relue au tour suivant.
  const end = bytes.lastIndexOf(10) + 1;
  // 200 : fichier entier (première lecture, ou serveur qui ignore Range).
  const whole = res.status === 200;
  if (whole ? end === state.offset : end === 0) return false;
  const added = parseJsonl<T>(new TextDecoder().decode(bytes.subarray(0, end))).games;
  state.rows = whole ? added : state.rows.concat(added);
  state.offset = (whole ? 0 : state.offset!) + end;
  return true;
}
