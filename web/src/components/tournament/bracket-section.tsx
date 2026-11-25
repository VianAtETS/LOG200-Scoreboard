import type { TBracketSide, TMatch } from '@/types/tournament.types';
import { useLayoutEffect, useRef, useState } from 'react';
import type { FC, ReactNode } from 'react';

type SectionProps = {
  side: TBracketSide;
  title: string;
  rounds: [string, TMatch[]][];
  children: (matches: TMatch[]) => ReactNode;
};

type TLinks = { width: number; height: number; paths: string[] };

const BracketSection: FC<SectionProps> = ({ title, rounds, children }) => {
  const ref = useRef<HTMLDivElement>(null);
  const [links, setLinks] = useState<TLinks>({ width: 0, height: 0, paths: [] });
  // Liaison coudée entre la carte d'un match et celle du match où va son vainqueur : mesurée après le rendu.
  useLayoutEffect(() => {
    const draw = (): void => {
      const node = ref.current;
      if (!node) {
        return;
      }
      const box = node.getBoundingClientRect();
      const paths: string[] = [];
      for (const target of node.querySelectorAll<HTMLElement>('.match-wrap[data-from]')) {
        const to = target.querySelector('.match')?.getBoundingClientRect();
        if (to === undefined || to.width === 0) {
          continue;
        }
        for (const id of (target.dataset['from'] ?? '').split(',').filter(Boolean)) {
          const from = node.querySelector(`[data-id="${id}"] .match`)?.getBoundingClientRect();
          if (from === undefined || from.width === 0) {
            continue;
          }
          const [ax, ay] = [from.right - box.left, from.top + from.height / 2 - box.top];
          const [bx, by] = [to.left - box.left, to.top + to.height / 2 - box.top];
          const mx = (ax + bx) / 2;
          paths.push(`M${ax},${ay} H${mx} V${by} H${bx}`);
        }
      }
      const next = { width: node.scrollWidth, height: node.scrollHeight, paths };
      setLinks((prev) =>
        prev.width === next.width &&
        prev.height === next.height &&
        prev.paths.join(',') === next.paths.join(',')
          ? prev
          : next,
      );
    };
    const frame = requestAnimationFrame(draw);
    const handleResize = (): void => {
      requestAnimationFrame(draw);
    };
    addEventListener('resize', handleResize);
    return () => {
      cancelAnimationFrame(frame);
      removeEventListener('resize', handleResize);
    };
  });
  return (
    <section className="bracket-section" aria-label={title}>
      <h3 className="band">{title}</h3>
      <div ref={ref} className="rounds">
        <svg className="links" aria-hidden="true" width={links.width} height={links.height}>
          {links.paths.map((path) => (
            <path key={path} d={path} />
          ))}
        </svg>
        {rounds.map(([name, list]) => (
          <div key={name} className="round">
            <p className="round-title">{name}</p>
            <div className="round-matches">{children(list)}</div>
          </div>
        ))}
      </div>
    </section>
  );
};

export { BracketSection };
