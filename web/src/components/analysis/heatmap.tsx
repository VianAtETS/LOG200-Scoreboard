import { useGetHeatmaps } from '@/dao/hooks/use-games';
import { SIZE, squareName } from '@/lib/replay';
import type { FC } from 'react';

const COLUMNS = Array.from({ length: SIZE }, (_, x) => String.fromCharCode(65 + x));

type HeatmapProps = { counts: Record<string, number>; color: string; label: string };

// Carte de chaleur de l'arène isola (arena/web/core.js, drawHeat) : plus la case est vive, plus elle a servi.
const Heatmap: FC<HeatmapProps> = ({ counts, color, label }) => {
  const max = Math.max(1, ...Object.values(counts));
  const cells = [];
  for (let y = 0; y < SIZE; y++) {
    cells.push(
      <div key={`row-${y}`} className="label">
        {SIZE - y}
      </div>
    );
    for (let x = 0; x < SIZE; x++) {
      const name = squareName(y * SIZE + x);
      const n = counts[name] ?? 0;
      // Même rampe qu'isola : 20 % pour une seule fois, 100 % pour la case la plus servie.
      const background = n ? `color-mix(in srgb, ${color} ${Math.round(20 + (80 * n) / max)}%, var(--color-tile))` : undefined;
      cells.push(<div key={name} className="cell" style={{ background }} title={`${name} : ${n.toLocaleString('fr-CA')}`} />);
    }
  }
  return (
    <figure className="heatmap">
      <div className="board" role="img" aria-label={`${label}, au plus ${max.toLocaleString('fr-CA')} fois sur une case`}>
        {cells}
        <div />
        {COLUMNS.map((c) => (
          <div key={`col-${c}`} className="label">
            {c}
          </div>
        ))}
      </div>
      <figcaption>{label}</figcaption>
    </figure>
  );
};

type BuildHeatmapsProps = { team: string; build?: string; color: string; name: string };

// Les deux cartes d'une version : où son pion est arrivé, quelles cases elle a détruites.
const BuildHeatmaps: FC<BuildHeatmapsProps> = ({ team, build, color, name }) => {
  const { data, error } = useGetHeatmaps(team);
  const map = data?.find((h) => h.build === (build ?? null));
  return (
    <div className="heat-side">
      <h3>
        {name}
        {map && `, ${map.games.toLocaleString('fr-CA')} partie${map.games > 1 ? 's' : ''}`}
      </h3>
      {error ? (
        <p>Cartes indisponibles : {error.message}</p>
      ) : !data ? (
        <p>Chargement des cartes…</p>
      ) : !map ? (
        <p>Aucune partie de cette version hors tournoi.</p>
      ) : (
        <div className="heat-maps">
          <Heatmap counts={map.moves} color={color} label="Arrivées du pion" />
          <Heatmap counts={map.blocks} color={color} label="Cases détruites" />
        </div>
      )}
    </div>
  );
};

export { BuildHeatmaps };
