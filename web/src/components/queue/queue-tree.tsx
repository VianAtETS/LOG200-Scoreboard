import {
  BELOW,
  COLUMN,
  ROW,
  STEP_MS,
  TILE,
  TOP,
  bend,
  describe,
  kinds,
  layout,
  short,
  signed,
} from '@/lib/queue';
import type { TPoint, TQueueMarks, TQueueNode } from '@/lib/queue';
import { cn } from '@/lib/utils';
import type { TQueueTeam } from '@/types/summary.types';
import { useEffectEvent, useLayoutEffect, useMemo, useRef } from 'react';
import type { CSSProperties, FC, RefObject } from 'react';

type Props = {
  teams: TQueueTeam[];
  marks: TQueueMarks;
  next: number;
  // Change avec la file, les parties ou les annonces : le dessin est refait.
  drawKey: string;
};

type TDepthStyle = CSSProperties & { '--d': number };
type TTreeStyle = CSSProperties & { '--trail': string; '--ignite': string };
const REVEAL_BASE_MS = 120;
const IGNITE_EXTRA_MS = 480;
const AXIS_BOTTOM_OFFSET = 16;
const AHEAD_OFFSET = 12;
const PAWN_INSET = 4;
const NEXT_LABEL_OFFSET = 15;
const AXIS_INSET = 8;
const AXIS_ARROW_BACK = 13;
const AXIS_ARROW_HEIGHT = 4;
const AXIS_LABEL_OFFSET = 13;
const depthStyle = (depth: number): TDepthStyle => ({ '--d': depth });

function geometry(
  nodes: TQueueNode[],
  at: Map<string, TPoint>,
  next: number,
): { center: (index: number) => TPoint; trail: string } {
  const center = (index: number): TPoint => {
    const node = nodes[index];
    const point = node === undefined ? undefined : at.get(node.id);
    if (point === undefined) {
      throw new Error('Missing queue position');
    }
    return point;
  };

  const chain: number[] = [];
  for (let index: number | null = next; index !== null; index = nodes[index]?.parent ?? null) {
    chain.unshift(index);
  }
  let trail = '';
  if (chain.length > 1) {
    trail = `M${center(chain[0] ?? next).x} ${center(chain[0] ?? next).y}`;
    for (let k = 1; k < chain.length; k++) {
      trail += bend(
        center(chain[k - 1] ?? next),
        center(chain[k] ?? next),
        center(chain[k] ?? next).y,
      );
    }
  }
  return { center, trail };
}

function animateQueue(
  root: SVGSVGElement,
  before: Map<string, TPoint>,
  at: Map<string, TPoint>,
): void {
  const moved: SVGGElement[] = [];
  for (const group of root.querySelectorAll<SVGGElement>('.node')) {
    const { team } = group.dataset;
    if (team === undefined) {
      continue;
    }
    const from = before.get(team);
    if (!from) {
      group.classList.add('fresh');
      continue;
    }
    const to = at.get(team);
    if (to === undefined) {
      continue;
    }
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
}

function useQueueAnimation(
  svg: RefObject<SVGSVGElement | null>,
  at: Map<string, TPoint>,
  drawKey: string,
): void {
  // Places du dessin précédent : chaque case glisse de l'ancienne vers la nouvelle.
  const previous = useRef<Map<string, TPoint> | null>(null);

  const runAnimation = useEffectEvent((_key: string) => {
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
    animateQueue(root, before, at);
  });

  // Le dessin est remonté à chaque changement (clé) : la file avance sous les yeux.
  useLayoutEffect(() => {
    runAnimation(drawKey);
  }, [drawKey]);
}

// Une colonne par équipe : l'arbre défile horizontalement au-delà d'une quinzaine.
const QueueTree: FC<Props> = ({ teams, marks, next, drawKey }) => {
  const svg = useRef<SVGSVGElement>(null);
  const nodes = layout(teams, next);
  const depth = Math.max(...nodes.map((node) => node.depth));
  const width = nodes.length * COLUMN;
  const height = TOP + depth * ROW + TILE.h + BELOW;
  const at = new Map(
    nodes.map((node, index) => [
      node.id,
      { x: index * COLUMN + COLUMN / 2, y: TOP + node.depth * ROW + TILE.h / 2 },
    ]),
  );
  const { center, trail } = geometry(nodes, at, next);
  const reveal = (depth + 1) * STEP_MS + REVEAL_BASE_MS;
  const axis = height - AXIS_BOTTOM_OFFSET;
  useQueueAnimation(svg, at, drawKey);
  const style = useMemo<TTreeStyle>(
    () => ({ '--trail': `${reveal}ms`, '--ignite': `${reveal + IGNITE_EXTRA_MS}ms` }),
    [reveal],
  );

  return (
    <svg
      key={drawKey}
      ref={svg}
      viewBox={`0 0 ${width} ${height}`}
      width={width}
      height={height}
      role="img"
      aria-label={`File d'attente, dans l'ordre : ${teams.map((team) => team.id).join(', ')}`}
      style={style}
    >
      <g className="edges">
        {nodes.map((node, i) => {
          if (node.parent === null) {
            return null;
          }
          const from = center(node.parent);
          const to = center(i);
          return (
            <path
              key={node.id}
              className="edge"
              d={`M${from.x} ${from.y + TILE.h / 2}${bend(from, to, to.y - TILE.h / 2)}`}
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
            <rect
              x={box.x}
              y={box.y}
              width={box.width}
              height={box.height}
              rx={box.rx}
              className="base"
            />
            <rect x={box.x} y={box.y} width={box.width} height={box.height} rx={box.rx} />
            <text className="name" x={x} y={y - 2}>
              {short(node.id)}
            </text>
            <text className="ahead" x={x} y={y + AHEAD_OFFSET}>
              {signed(node.ahead)}
            </text>
            {pawn && (
              <circle
                className={cn('pawn', pawn)}
                cx={x + TILE.w / 2 - PAWN_INSET}
                cy={y - TILE.h / 2 + PAWN_INSET}
                r={6}
              />
            )}
            {i === next && (
              <text className="next-label" x={x} y={y + TILE.h / 2 + NEXT_LABEL_OFFSET}>
                prochaine
              </text>
            )}
          </g>
        );
      })}
      <path
        className="axis"
        d={`M${AXIS_INSET} ${axis}H${width - AXIS_INSET}M${width - AXIS_ARROW_BACK} ${axis - AXIS_ARROW_HEIGHT}L${width - AXIS_INSET} ${axis}L${width - AXIS_ARROW_BACK} ${axis + AXIS_ARROW_HEIGHT}`}
      />
      <text className="axis-label" x={AXIS_INSET} y={axis + AXIS_LABEL_OFFSET}>
        en retard
      </text>
      <text className="axis-label end" x={width - AXIS_INSET} y={axis + AXIS_LABEL_OFFSET}>
        en avance
      </text>
    </svg>
  );
};

export { QueueTree };
