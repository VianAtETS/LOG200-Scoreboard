import {
  BELOW,
  bend,
  COLUMN,
  describe,
  kinds,
  layout,
  ROW,
  short,
  signed,
  STEP_MS,
  TILE,
  TOP,
} from '@/lib/queue';
import type { TPoint, TQueueMarks } from '@/lib/queue';
import { cn } from '@/lib/utils';
import type { TQueueTeam } from '@/types/summary.types';
import { useLayoutEffect, useRef } from 'react';
import type { CSSProperties, FC } from 'react';

type Props = {
  teams: TQueueTeam[];
  marks: TQueueMarks;
  next: number;
  // Change avec la file, les parties ou les annonces : le dessin est refait.
  drawKey: string;
};

const depthStyle = (d: number) => ({ '--d': d }) as CSSProperties;

// Une colonne par équipe : l'arbre défile horizontalement au-delà d'une quinzaine.
const QueueTree: FC<Props> = ({ teams, marks, next, drawKey }) => {
  const svg = useRef<SVGSVGElement>(null);
  // Places du dessin précédent : chaque case glisse de l'ancienne vers la nouvelle.
  const previous = useRef<Map<string, TPoint> | null>(null);

  const nodes = layout(teams, next);
  const depth = Math.max(...nodes.map((n) => n.depth));
  const width = nodes.length * COLUMN;
  const height = TOP + depth * ROW + TILE.h + BELOW;
  const at = new Map(
    nodes.map((n, i) => [
      n.id,
      { x: i * COLUMN + COLUMN / 2, y: TOP + n.depth * ROW + TILE.h / 2 },
    ]),
  );
  const center = (i: number) => at.get(nodes[i].id)!;
  const reveal = (depth + 1) * STEP_MS + 120;

  const chain: number[] = [];
  for (let i: number | null = next; i !== null; i = nodes[i].parent) {
    chain.unshift(i);
  }
  let trail = '';
  if (chain.length > 1) {
    trail = `M${center(chain[0]).x} ${center(chain[0]).y}`;
    for (let k = 1; k < chain.length; k++) {
      trail += bend(center(chain[k - 1]), center(chain[k]), center(chain[k]).y);
    }
  }
  const axis = height - 16;

  // Le dessin est remonté à chaque changement (clé) : la file avance sous les yeux.
  useLayoutEffect(() => {
    const root = svg.current;
    if (!root) {
      return;
    }
    const before = previous.current;
    previous.current = at;
    if (!before) {
      root.classList.add('enter');
      return;
    }
    const moved: SVGGElement[] = [];
    for (const group of root.querySelectorAll<SVGGElement>('.node')) {
      const team = group.dataset.team!;
      const from = before.get(team);
      if (!from) {
        group.classList.add('fresh');
        continue;
      }
      const to = at.get(team)!;
      group.style.transition = 'none';
      group.style.transform = `translate(${from.x - to.x}px, ${from.y - to.y}px)`;
      moved.push(group);
    }
    root.getBoundingClientRect();
    for (const group of moved) {
      group.style.transition = '';
      group.style.transform = '';
    }
    root.classList.add('moving');
    // `at` suit `drawKey`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [drawKey]);

  return (
    <svg
      key={drawKey}
      ref={svg}
      viewBox={`0 0 ${width} ${height}`}
      width={width}
      height={height}
      role="img"
      aria-label={`File d'attente, dans l'ordre : ${teams.map((t) => t.id).join(', ')}`}
      style={{ '--trail': `${reveal}ms`, '--ignite': `${reveal + 480}ms` } as CSSProperties}
    >
      <g className="edges">
        {nodes.map((node, i) => {
          if (node.parent === null) {
            return null;
          }
          const a = center(node.parent);
          const b = center(i);
          return (
            <path
              key={node.id}
              className="edge"
              d={`M${a.x} ${a.y + TILE.h / 2}${bend(a, b, b.y - TILE.h / 2)}`}
              pathLength={1}
              style={depthStyle(node.depth - 1)}
            />
          );
        })}
      </g>
      {trail && <path className="trail" d={trail} pathLength={1} />}
      {nodes.map((node, i) => {
        const { x, y } = center(i);
        const pawn = marks.busy.get(node.id);
        const box = { x: x - TILE.w / 2, y: y - TILE.h / 2, width: TILE.w, height: TILE.h, rx: 5 };
        return (
          <g
            key={node.id}
            className={cn('node', ...kinds(node, marks, i === next), node.path && 'path')}
            data-team={node.id}
            style={depthStyle(node.depth)}
          >
            <title>{describe(node, marks)}</title>
            <rect {...box} className="base" />
            <rect {...box} />
            <text className="name" x={x} y={y - 2}>
              {short(node.id)}
            </text>
            <text className="ahead" x={x} y={y + 12}>
              {signed(node.ahead)}
            </text>
            {pawn && (
              <circle
                className={cn('pawn', pawn)}
                cx={x + TILE.w / 2 - 4}
                cy={y - TILE.h / 2 + 4}
                r={6}
              />
            )}
            {i === next && (
              <text className="next-label" x={x} y={y + TILE.h / 2 + 15}>
                prochaine
              </text>
            )}
          </g>
        );
      })}
      <path
        className="axis"
        d={`M8 ${axis}H${width - 8}M${width - 13} ${axis - 4}L${width - 8} ${axis}L${width - 13} ${axis + 4}`}
      />
      <text className="axis-label" x={8} y={axis + 13}>
        en retard
      </text>
      <text className="axis-label end" x={width - 8} y={axis + 13}>
        en avance
      </text>
    </svg>
  );
};

export { QueueTree };
