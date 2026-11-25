import type { TRunningGame } from '@/types/games.types';
import type { TModel } from '@/types/summary.types';
import type { TBracketSide, TMatch, TTournament } from '@/types/tournament.types';
import { useLayoutEffect, useRef, useState, type FC, type ReactNode } from 'react';
import { MatchCard } from './match-card';
import { roundTitle, SIDES } from './tournament.utils';

type Props = {
  view: TTournament;
  model: TModel | null | undefined;
  running: TRunningGame[];
  describe: (team: string) => string;
};

// Tableau à double élimination : le principal (avec la grande finale), puis le repêchage.
const Bracket: FC<Props> = ({ view, model, running, describe }) => {
  const seeds = new Map(view.entrants.map((e, i) => [e.team, i + 1]));
  const byId = new Map(view.matches.map((m) => [m.id, m]));

  return (
    <div id="bracket" className="bracket">
      {SIDES.map(([side, title]) => {
        const matches = view.matches.filter((m) => m.side === side);
        const last = Math.max(...matches.map((m) => m.round));
        const rounds: [string, TMatch[]][] = [];
        for (let r = 1; r <= last; r++)
          rounds.push([roundTitle(side, r, last), matches.filter((m) => m.round === r)]);
        if (side === 'winners')
          rounds.push([roundTitle('final'), view.matches.filter((m) => m.side === 'final')]);
        return (
          <BracketSection key={side} side={side} title={title} rounds={rounds}>
            {(list) =>
              list.map((m) => (
                <MatchCard
                  key={m.id}
                  match={m}
                  view={view}
                  seeds={seeds}
                  byId={byId}
                  model={model}
                  running={running}
                  describe={describe}
                />
              ))
            }
          </BracketSection>
        );
      })}
    </div>
  );
};

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
    const draw = () => {
      const node = ref.current;
      if (!node) return;
      const box = node.getBoundingClientRect();
      const paths: string[] = [];
      for (const target of node.querySelectorAll<HTMLElement>('.match-wrap[data-from]')) {
        const to = target.querySelector('.match')?.getBoundingClientRect();
        if (!to?.width) continue;
        for (const id of target.dataset.from!.split(',').filter(Boolean)) {
          const from = node.querySelector(`[data-id="${id}"] .match`)?.getBoundingClientRect();
          if (!from?.width) continue;
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
        prev.paths.join() === next.paths.join()
          ? prev
          : next,
      );
    };
    const frame = requestAnimationFrame(draw);
    const handleResize = () => requestAnimationFrame(draw);
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
          {links.paths.map((d) => (
            <path key={d} d={d} />
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

export { Bracket };
