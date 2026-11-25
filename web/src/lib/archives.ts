// Les versions restent celles de summary.builds : équipe + hash, jamais le seul nom d'équipe.
import type { TColor, TGameRow } from '@/types/games.types';
import type { TSummary, TTeamRating } from '@/types/summary.types';
import { isRemote, ratingOf } from './elo.ts';

export type TArchiveOption = {
  team: string;
  build: string;
  version: number;
  key: string;
  label: string;
};

export type TRankingRow = TTeamRating & { archived?: boolean };

export const archiveKey = (team: string, build?: string): string =>
  `${encodeURIComponent(team)}@${build}`;

export function currentBuilds(summary: TSummary): Map<string, string | undefined> {
  const latest = new Map<string, TTeamRating>();
  for (const scope of Object.values(summary.scopes)) {
    for (const row of scope.ratings) {
      const previous = latest.get(row.team);
      if (
        row.build !== undefined &&
        row.build !== '' &&
        (previous === undefined ||
          (row.last !== undefined && previous.last !== undefined && row.last > previous.last))
      ) {
        latest.set(row.team, row);
      }
    }
  }
  return new Map(
    Object.entries(summary.builds).map(([team, builds]) => [
      team,
      latest.get(team)?.build ?? builds.at(-1),
    ]),
  );
}

export function archiveOptions(summary: TSummary): TArchiveOption[] {
  const current = currentBuilds(summary);
  return Object.entries(summary.builds).flatMap(([team, builds]) =>
    builds.flatMap((build, i) =>
      build === current.get(team)
        ? []
        : [
            {
              team,
              build,
              version: i + 1,
              key: archiveKey(team, build),
              label: `${team} v${i + 1}`,
            },
          ],
    ),
  );
}

function updateArchive(entry: TRankingRow, game: TGameRow, color: TColor, rating: number): void {
  const row = entry;
  row.rating = rating;
  row.peak = Math.max(entry.peak ?? Number.NaN, rating);
  row.games += 1;
  row.build_games += 1;
  if (game.winner === color) {
    row.wins += 1;
    row.build_wins += 1;
  } else {
    row.losses += 1;
  }
  row.remote ||= Boolean(game[color].remote);
  row.last = game.ts;
}

// Dernier Elo réellement enregistré pour ce build, sans recalculer le classement actuel.
export function archivedRows(
  rows: readonly TGameRow[],
  selected: TArchiveOption[],
  { session = '', verified = false }: { session?: string; verified?: boolean } = {},
): TRankingRow[] {
  const wanted = new Map(selected.map((text) => [text.key, text]));
  const out = new Map<string, TRankingRow>();
  for (const game of rows.toSorted((left, right) => left.ts.localeCompare(right.ts))) {
    if (session === '' ? Boolean(game.tournament) : game.session !== session) {
      continue;
    }
    if (verified && isRemote(game)) {
      continue;
    }
    for (const color of ['red', 'black'] as TColor[]) {
      const side = game[color];
      const id = archiveKey(side.team, side.build);
      const option = wanted.get(id);
      if (!option) {
        continue;
      }
      const [rating, delta] = ratingOf(side, session ? 'session' : 'all', verified);
      const entry = out.get(id) ?? {
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
      updateArchive(entry, game, color, rating);
      out.set(id, entry);
    }
  }
  return [...out.values()];
}

// Anciennes versions placées à leur Elo dans le classement (déjà trié par Elo), sans rang actif, avec
// leurs temps par coup publiés sur la ligne de leur équipe.
export function withArchivedRows(ratings: TTeamRating[], archived: TRankingRow[]): TRankingRow[] {
  const teams = new Map(ratings.map((rating) => [rating.team, rating]));
  const extra = archived
    .flatMap((old) => {
      const team = teams.get(old.team);
      if (team?.build === old.build) {
        return [];
      }
      const pace = team?.archived_paces?.[String(old.build)];
      return [pace ? { ...old, pace } : old];
    })
    .toSorted((left, right) => right.rating - left.rating);
  const out: TRankingRow[] = [];
  let i = 0;
  for (const rating of ratings) {
    let next = extra[i];
    while (next !== undefined && next.rating > rating.rating) {
      out.push(next);
      i += 1;
      next = extra[i];
    }
    out.push(rating);
  }
  return [...out, ...extra.slice(i)];
}
