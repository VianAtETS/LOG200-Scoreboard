// Les versions restent celles de summary.builds : équipe + hash, jamais le seul nom d'équipe.
import type { TColor, TGameRow } from '../types/games.types';
import type { TSummary, TTeamRating } from '../types/summary.types';
import { isRemote, ratingOf } from './elo.ts';

export type TArchiveOption = { team: string; build: string; version: number; key: string; label: string };

export type TRankingRow = TTeamRating & { archived?: boolean };

export const archiveKey = (team: string, build?: string) => `${encodeURIComponent(team)}@${build}`;

export function currentBuilds(summary: TSummary) {
  const latest = new Map<string, TTeamRating>();
  for (const scope of Object.values(summary.scopes))
    for (const r of scope.ratings) {
      if (r.build && (!latest.has(r.team) || r.last! > latest.get(r.team)!.last!)) latest.set(r.team, r);
    }
  return new Map(
    Object.entries(summary.builds).map(([team, builds]) => [team, latest.get(team)?.build ?? builds.at(-1)])
  );
}

export function archiveOptions(summary: TSummary): TArchiveOption[] {
  const current = currentBuilds(summary);
  return Object.entries(summary.builds).flatMap(([team, builds]) =>
    builds.flatMap((build, i) =>
      build === current.get(team)
        ? []
        : [{ team, build, version: i + 1, key: archiveKey(team, build), label: `${team} v${i + 1}` }]
    )
  );
}

// Dernier Elo réellement enregistré pour ce build, sans recalculer le classement actuel.
export function archivedRows(
  rows: readonly TGameRow[],
  selected: TArchiveOption[],
  { session = '', verified = false }: { session?: string; verified?: boolean } = {}
): TRankingRow[] {
  const wanted = new Map(selected.map((s) => [s.key, s]));
  const out = new Map<string, TRankingRow>();
  for (const g of [...rows].sort((a, b) => a.ts.localeCompare(b.ts))) {
    if (session ? g.session !== session : g.tournament) continue;
    if (verified && isRemote(g)) continue;
    for (const color of ['red', 'black'] as TColor[]) {
      const side = g[color];
      const id = archiveKey(side.team, side.build);
      const option = wanted.get(id);
      if (!option) continue;
      const [rating, delta] = ratingOf(side, session ? 'session' : 'all', verified);
      const r = out.get(id) ?? {
        ...option,
        rating,
        peak: rating - delta,
        games: 0,
        wins: 0,
        losses: 0,
        build_games: 0,
        build_wins: 0,
        archived: true,
        remote: false,
      };
      r.rating = rating;
      r.peak = Math.max(r.peak!, rating);
      r.games++;
      r.build_games++;
      if (g.winner === color) {
        r.wins++;
        r.build_wins++;
      } else r.losses++;
      r.remote ||= Boolean(side.remote);
      r.last = g.ts;
      out.set(id, r);
    }
  }
  return [...out.values()];
}

// Garder une ancienne version près de son équipe, sans lui attribuer un rang actif.
export function withArchivedRows(ratings: TTeamRating[], archived: TRankingRow[]): TRankingRow[] {
  const current = new Map(ratings.map((r) => [r.team, r.build]));
  const extra = archived.filter((old) => current.get(old.team) !== old.build);
  return ratings
    .flatMap((r) => [r, ...extra.filter((old) => old.team === r.team)])
    .concat(extra.filter((old) => !current.has(old.team)));
}
