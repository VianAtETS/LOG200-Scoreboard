// File d'attente du ladder (queue.json) : l'arbre, ou une liste au-delà de LIST_FROM équipes.

import { CloseIcon } from '@/components/icons/icons';
import { useGetQueue } from '@/dao/hooks/use-ladder';
import { useInterval } from '@/hooks/use-interval';
import { countdown, describe, kinds, LIST_FROM, signed } from '@/lib/queue';
import type { TQueueMarks } from '@/lib/queue';
import { cn } from '@/lib/utils';
import type { TColor, TRunningGame } from '@/types/games.types';
import type { TAnnounced, TQueueTeam } from '@/types/summary.types';
import { Fragment, useEffect, useRef, useState } from 'react';
import type { FC, ReactNode } from 'react';
import { QueueTree } from './queue-tree';

type Props = {
  open: boolean;
  onClose: () => void;
  running: TRunningGame[];
};

const QueueDialog: FC<Props> = ({ open, onClose, running }) => {
  const dialog = useRef<HTMLDialogElement>(null);
  const { data: file, isFetched } = useGetQueue(open);

  useEffect(() => {
    if (open && !dialog.current?.open) {
      dialog.current?.showModal();
    }
  }, [open]);

  return (
    <dialog
      ref={dialog}
      id="queue"
      className="queue"
      aria-labelledby="queue-title"
      aria-describedby="queue-lede"
      onClose={onClose}
    >
      {/* Démonté à la fermeture : l'entrée se rejoue à la prochaine ouverture. */}
      {open && isFetched ? (
        <QueueView teams={file?.teams ?? null} running={running} announced={file?.next ?? []} />
      ) : (
        <QueueFrame lede={null} shape={undefined}>
          {null}
        </QueueFrame>
      )}
    </dialog>
  );
};

type ViewProps = {
  teams: TQueueTeam[] | null;
  running: TRunningGame[];
  announced: TAnnounced[];
};

const QueueView: FC<ViewProps> = ({ teams, running, announced }) => {
  const [now, setNow] = useState(() => Date.now());
  useInterval(() => setNow(Date.now()), announced.length > 0 ? 1000 : null);

  if (!teams?.length) {
    const text = teams
      ? 'Aucune équipe en file pour le moment.'
      : "Ce ladder ne publie pas encore sa file d'attente.";
    return (
      <QueueFrame lede={null}>
        <p className="queue-empty">{text}</p>
      </QueueFrame>
    );
  }

  const busy = new Map<string, TColor>(
    running.flatMap((g) => [[g.red, 'red'] as const, [g.black, 'black'] as const]),
  );
  const called = new Set(announced.flatMap((a) => a.teams));
  const marks: TQueueMarks = { busy, called };
  const free = teams.filter((t) => !busy.has(t.id) && !called.has(t.id));
  const next = Math.max(0, teams.indexOf(free[0]));
  const after = free.slice(0, 3).map((t) => t.id);
  const shape = teams.length >= LIST_FROM ? 'list' : 'tree';
  const drawKey = JSON.stringify([teams, [...busy], announced]);

  return (
    <QueueFrame
      lede={<Headline announced={announced} after={after} games={running.length} now={now} />}
      shape={shape}
    >
      {shape === 'list' ? (
        <QueueList teams={teams} marks={marks} next={next} />
      ) : (
        <QueueTree teams={teams} marks={marks} next={next} drawKey={drawKey} />
      )}
    </QueueFrame>
  );
};

type FrameProps = {
  lede: ReactNode;
  shape?: 'list' | 'tree';
  children: ReactNode;
};

const QueueFrame: FC<FrameProps> = ({ lede, shape, children }) => (
  <>
    <header className="queue-head">
      <div>
        <h2 id="queue-title">File d'attente</h2>
        <p id="queue-lede" className="queue-lede">
          {lede}
        </p>
      </div>
      <form method="dialog">
        <button className="icon queue-close" type="submit" aria-label="Fermer la file d'attente">
          <CloseIcon />
        </button>
      </form>
    </header>
    <div
      id="queue-tree"
      className="queue-tree"
      tabIndex={0}
      aria-label="Arbre de la file d'attente"
      data-shape={shape}
    >
      {children}
    </div>
    <QueueKey />
  </>
);

// Les mêmes cases, dans l'ordre de la file, de gauche à droite puis de haut en bas.
const QueueList: FC<{ teams: TQueueTeam[]; marks: TQueueMarks; next: number }> = ({
  teams,
  marks,
  next,
}) => (
  <ol className="queue-list">
    {teams.map((team, i) => {
      const pawn = marks.busy.get(team.id);
      return (
        <li
          key={team.id}
          className={cn(...kinds(team, marks, i === next))}
          title={describe(team, marks)}
        >
          <span className="name">{team.id}</span>
          <span className="ahead">{signed(team.ahead)}</span>
          {pawn && <span className={cn('pawn', pawn)} />}
        </li>
      );
    })}
  </ol>
);

const Names: FC<{ names: string[] }> = ({ names }) => (
  <>
    {names.map((name, i) => (
      <Fragment key={name}>
        {i > 0 && (i === names.length - 1 ? ' et ' : ', ')}
        <strong>{name}</strong>
      </Fragment>
    ))}
  </>
);

// Phrase d'en-tête : les paires annoncées et leur compte à rebours, puis les suivantes dans la file.
const Headline: FC<{ announced: TAnnounced[]; after: string[]; games: number; now: number }> = ({
  announced,
  after,
  games,
  now,
}) => {
  const sorted = [...announced].sort((a, b) => Date.parse(a.at) - Date.parse(b.at));
  return (
    <>
      {sorted.map(({ teams: [a, b], at }, i) => (
        <Fragment key={`${a}-${b}`}>
          {i ? ', ' : 'Annoncé : '}
          <strong>{a}</strong>
          {' contre '}
          <strong>{b}</strong>
          {` ${countdown(at, now)}`}
        </Fragment>
      ))}
      {sorted.length > 0 && '. '}
      {after.length > 0 && (
        <>
          {sorted.length > 0 ? 'Ensuite : ' : 'Prochaines à jouer : '}
          <Names names={after} />.
        </>
      )}
      {after.length === 0 && sorted.length === 0 && 'Toutes les équipes sont en partie.'}
      {games > 0 && ` ${games} partie${games > 1 ? 's' : ''} en cours.`}
    </>
  );
};

const KeyTile: FC<{ kind: string; pawns?: boolean }> = ({ kind, pawns }) => (
  <svg viewBox="0 0 34 22" aria-hidden="true">
    <g className={`node ${kind}`}>
      <rect x="1" y="1" width="32" height="20" rx="4" />
      {pawns && (
        <>
          <circle className="pawn red" cx="9" cy="11" r="5" />
          <circle className="pawn black" cx="24" cy="11" r="5" />
        </>
      )}
    </g>
  </svg>
);

const QueueKey: FC = () => (
  <div className="queue-foot">
    <dl className="queue-key">
      <div>
        <dt>
          <KeyTile kind="team next" />
        </dt>
        <dd>Prochaine équipe libre à jouer</dd>
      </div>
      <div>
        <dt>
          <KeyTile kind="team called" />
        </dt>
        <dd>Annoncée : joue dès la fin de la pause de son slot</dd>
      </div>
      <div>
        <dt>
          <KeyTile kind="team" />
        </dt>
        <dd>Équipe</dd>
      </div>
      <div>
        <dt>
          <KeyTile kind="bot" />
        </dt>
        <dd>Bot, qui avance deux fois plus vite</dd>
      </div>
      <div>
        <dt>
          <KeyTile kind="team busy" pawns />
        </dt>
        <dd>En partie, avec son pion</dd>
      </div>
      <div>
        <dt>
          <span className="queue-sample">+2</span>
        </dt>
        <dd>Avance sur l'horloge de la file, en parties</dd>
      </div>
    </dl>
    <div className="queue-notes">
      <p>
        Chaque paire lancée fait avancer ses deux équipes de 2, un bot de 4. Une nouvelle équipe
        entre tout à gauche. Le match suivant est tiré au début de la pause d'un slot, d'où
        l'annonce exacte ; elle tombe si une équipe soumet un nouveau jar ou part en tournoi.
        Au-delà, la file dit qui joue bientôt, pas contre qui.
      </p>
      <p className="queue-fine">
        Arbre binaire calculé pour l'affichage, dans l'ordre de la file. En mémoire, la file est un
        B-tree plat.
      </p>
    </div>
  </div>
);

export { QueueDialog };
