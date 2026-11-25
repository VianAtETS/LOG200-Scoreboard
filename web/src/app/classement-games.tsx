import { GLOBAL } from '@/lib/constants';
import { BETWEEN } from '@/lib/elo';
import { pageUrl } from '@/lib/url';
import { SectionToggle } from '@/components/controls/section-toggle';
import { WindowRadio } from '@/components/controls/window-radio';
import { DuelRecord } from '@/components/games/duel-record';
import { GamesList } from '@/components/games/games-list';
import { TeamPicker } from '@/components/picker/team-picker';
import type { TReplay, TReplayActions } from '@/components/replay/use-replay';
import type { TGameRow } from '@/types/games.types';
// oxlint-disable-next-line import/max-dependencies -- Recent games composes filters, picker, replay and list.
import { useCallback } from 'react';
import type { ChangeEvent, JSX } from 'react';
import type { useClassementData } from './classement-data';

const MATCH_OPTIONS = [
  { value: GLOBAL, label: "Avec l'une" },
  { value: BETWEEN, label: 'Entre elles' },
];

type TProps = {
  data: ReturnType<typeof useClassementData>;
  replay: TReplay;
  actions: TReplayActions;
  sideOrder: 'left' | 'right' | 'color';
  match: string;
  collapsed: boolean;
  onToggle: () => void;
  chooseTeams: (next: string[]) => void;
  changeMatch: (value: string) => void;
  changeSide: (event: ChangeEvent<HTMLSelectElement>) => void;
};

export function RecentGames({
  data,
  replay,
  actions,
  sideOrder,
  match,
  collapsed,
  onToggle,
  chooseTeams,
  changeMatch,
  changeSide,
}: TProps): JSX.Element {
  const { ratings, teams, duelGames, version, list, rating } = data;
  const selectGame = useCallback(
    (game: TGameRow): void => {
      void actions.showGame(game, true);
    },
    [actions],
  );
  return (
    <section id="games-section" className="section games" aria-labelledby="games-title">
      <SectionToggle
        id="games-title"
        collapsed={collapsed}
        controls="games-section"
        onToggle={onToggle}
      >
        Parties récentes
      </SectionToggle>
      <TeamPicker
        // oxlint-disable-next-line react/forbid-component-props -- This class selects the existing games-panel layout.
        className="games-picker"
        ratings={ratings}
        selected={teams}
        onChange={chooseTeams}
        placeholder="Filtrer par équipe"
        chipsLabel="Équipes choisies"
      />
      {teams.length > 0 && (
        <div className="games-controls">
          {teams.length === 1 ? (
            <label className="side-order" htmlFor="game-side-order">
              <span>Position de {teams[0]}</span>
              <select id="game-side-order" value={sideOrder} onChange={changeSide}>
                <option value="color">Ordre des couleurs</option>
                <option value="left">Toujours à gauche</option>
                <option value="right">Toujours à droite</option>
              </select>
            </label>
          ) : (
            <WindowRadio
              id="match"
              name="match"
              label="Parties montrées"
              value={match}
              options={MATCH_OPTIONS}
              onChange={changeMatch}
            />
          )}
        </div>
      )}
      <a id="graph-link" className="graph-link" href={pageUrl('graphe.html')}>
        {teams.length > 0
          ? 'Voir les rangs et l’Elo des équipes choisies'
          : 'Voir les graphes du classement'}
      </a>
      <DuelRecord teams={teams} games={duelGames} ratings={ratings} versionOf={version} />
      <GamesList
        games={list}
        rating={rating}
        currentId={replay.id}
        selectedTeam={teams.length === 1 ? teams[0] : undefined}
        sideOrder={sideOrder}
        onSelect={selectGame}
      />
    </section>
  );
}
