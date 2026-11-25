import { parseJsonl } from './elo.ts';

const RANGE_NOT_SATISFIABLE = 416;
const NEWLINE = 10;
const OK = 200;
export type TTailState<T> = { offset: number | null; rows: T[] };

export const tailState = <T>(): TTailState<T> => ({ offset: null, rows: [] });

// JSONL en ajout seul : lu en entier, puis seulement la suite (Range). Vrai si des lignes sont arrivées.
export async function tail<T extends { v: number }>(
  url: string,
  state: TTailState<T>,
): Promise<boolean> {
  const current = state;
  const headers: HeadersInit =
    state.offset !== null && state.offset !== 0 && !Number.isNaN(state.offset)
      ? { Range: `bytes=${state.offset}-` }
      : {};
  const res = await fetch(url, { cache: 'no-store', headers });
  if (res.status === RANGE_NOT_SATISFIABLE) {
    // Rien après offset, sauf si le fichier a rétréci (réécrit) : on relit tout.
    if (Number(res.headers.get('Content-Range')?.split('/')[1]) < (state.offset ?? 0)) {
      current.offset = 0;
      return tail(url, state);
    }
    return false;
  }
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }
  const bytes = new Uint8Array(await res.arrayBuffer());
  // Une dernière ligne en cours d'écriture est relue au tour suivant.
  const end = bytes.lastIndexOf(NEWLINE) + 1;
  // 200 : fichier entier (première lecture, ou serveur qui ignore Range).
  const whole = res.status === OK;
  if (whole ? end === state.offset : end === 0) {
    return false;
  }
  const added = parseJsonl<T>(new TextDecoder().decode(bytes.subarray(0, end))).games;
  current.rows = whole ? added : [...state.rows, ...added];
  current.offset = (whole ? 0 : (state.offset ?? 0)) + end;
  return true;
}
