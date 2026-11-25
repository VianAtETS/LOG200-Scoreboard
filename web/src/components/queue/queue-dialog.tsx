// File d'attente du ladder (queue.json) : l'arbre, ou une liste au-delà de LIST_FROM équipes.

import { CloseIcon } from '@/components/icons/icons';
import { useGetQueue } from '@/dao/hooks/use-ladder';
import { useInterval } from '@/hooks/use-interval';
import { LIST_FROM, countdown, describe, kinds, signed } from '@/lib/queue';
import type { TQueueMarks } from '@/lib/queue';
import { cn } from '@/lib/utils';
import type { TColor, TRunningGame } from '@/types/games.types';
import type { TAnnounced, TQueueTeam } from '@/types/summary.types';
import { Fragment, useEffect, useMemo, useRef, useState } from 'react';
import type { FC, ReactNode } from 'react';
import { QueueTree } from './queue-tree';

const NO_ANNOUNCEMENTS: TAnnounced[] = [];
const UPCOMING_COUNT = 3;
type Props = {
  open: boolean;
  onClose: () => void;
  running: TRunningGame[];
};

type ViewProps = {
  teams: TQueueTeam[] | null;
  running: TRunningGame[];
  announced: TAnnounced[];
};

type FrameProps = {
  lede: ReactNode;
  shape?: 'list' | 'tree';
  children: ReactNode;
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

// oxlint-disable-next-line react/no-multi-comp -- renders the dialog legend.
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
        <dd>Avance sur l&apos;horloge de la file, en parties</dd>
      </div>
    </dl>
    <div className="queue-notes">
      <p>
        Chaque paire lancée fait avancer ses deux équipes de 2, un bot de 4. Une nouvelle équipe
        entre tout à gauche. Le match suivant est tiré au début de la pause d&apos;un slot,
        d&apos;où l&apos;annonce exacte ; elle tombe si une équipe soumet un nouveau jar ou part en
        tournoi. Au-delà, la file dit qui joue bientôt, pas contre qui.
      </p>
      <p className="queue-fine">
        Arbre binaire calculé pour l&apos;affichage, dans l&apos;ordre de la file. En mémoire, la
        file est un B-tree plat.
      </p>
    </div>
  </div>
);

// oxlint-disable-next-line react/no-multi-comp -- provides the dialog shell for both loading and queue views.
const QueueFrame: FC<FrameProps> = ({ lede, shape, children }) => (
  <>
    <header className="queue-head">
      <div>
        <h2 id="queue-title">File d&apos;attente</h2>
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
      // Keep the scrollable queue reachable for keyboard users.
      // oxlint-disable-next-line jsx-a11y/no-noninteractive-tabindex
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
// oxlint-disable-next-line react/no-multi-comp -- renders the dialog’s alternate large-queue view.
const QueueList: FC<{ teams: TQueueTeam[]; marks: TQueueMarks; next: number }> = ({
  teams,
  marks,
  next,
}) => (
  <ol className="queue-list">
    {teams.map((team, index) => {
      const pawn = marks.busy.get(team.id);
      return (
        <li
          key={team.id}
          className={cn(...kinds(team, marks, index === next))}
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

// oxlint-disable-next-line react/no-multi-comp -- formats the queue headline.
const Names: FC<{ names: string[] }> = ({ names }) => (
  <>
    {names.map((name, index) => (
      <Fragment key={name}>
        {index > 0 && (index === names.length - 1 ? ' et ' : ', ')}
        <strong>{name}</strong>
      </Fragment>
    ))}
  </>
);

// Phrase d'en-tête : les paires annoncées et leur compte à rebours, puis les suivantes dans la file.
// oxlint-disable-next-line react/no-multi-comp -- renders the dialog’s queue summary.
const Headline: FC<{ announced: TAnnounced[]; after: string[]; games: number; now: number }> = ({
  announced,
  after,
  games,
  now,
}) => {
  const sorted = announced.toSorted((left, right) => Date.parse(left.at) - Date.parse(right.at));
  return (
    <>
      {sorted.map(({ teams: [first, second], at }, index) => (
        <Fragment key={`${first}-${second}`}>
          {index > 0 ? ', ' : 'Annoncé : '}
          <strong>{first}</strong>
          {' contre '}
          <strong>{second}</strong>
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

// oxlint-disable-next-line react/no-multi-comp -- renders this dialog’s fetched queue state.
const QueueView: FC<ViewProps> = ({ teams, running, announced }) => {
  const [now, setNow] = useState(() => Date.now());
  useInterval(() => setNow(Date.now()), announced.length > 0 ? 1000 : null);
  const { marks, next, after, drawKey } = useMemo(() => {
    const busy = new Map<string, TColor>(
      running.flatMap((game) => [[game.red, 'red'] as const, [game.black, 'black'] as const]),
    );
    const called = new Set(announced.flatMap((announcement) => announcement.teams));
    const free = (teams ?? []).filter((team) => !busy.has(team.id) && !called.has(team.id));
    return {
      marks: { busy, called },
      next: free[0] === undefined ? 0 : Math.max(0, (teams ?? []).indexOf(free[0])),
      after: free.slice(0, UPCOMING_COUNT).map((team) => team.id),
      drawKey: JSON.stringify([teams, [...busy], announced]),
    };
  }, [teams, running, announced]);
  const lede = useMemo(
    () => <Headline announced={announced} after={after} games={running.length} now={now} />,
    [announced, after, running.length, now],
  );
  const text =
    teams === null
      ? "Ce ladder ne publie pas encore sa file d'attente."
      : 'Aucune équipe en file pour le moment.';
  const shape = (teams?.length ?? 0) >= LIST_FROM ? 'list' : 'tree';
  return teams === null || teams.length === 0 ? (
    <QueueFrame lede={null}>
      <p className="queue-empty">{text}</p>
    </QueueFrame>
  ) : (
    <QueueFrame lede={lede} shape={shape}>
      {shape === 'list' ? (
        <QueueList teams={teams} marks={marks} next={next} />
      ) : (
        <QueueTree teams={teams} marks={marks} next={next} drawKey={drawKey} />
      )}
    </QueueFrame>
  );
};

// oxlint-disable-next-line react/no-multi-comp -- owns this dialog and its private views.
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
        <QueueView
          teams={file?.teams ?? null}
          running={running}
          announced={file?.next ?? NO_ANNOUNCEMENTS}
        />
      ) : (
        <QueueFrame lede={null}>{null}</QueueFrame>
      )}
    </dialog>
  );
};

export { QueueDialog };
