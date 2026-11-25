import type { pairCounts } from '@/lib/duel';
import type { TDuelPair } from '@/types/games.types';
import type { JSX } from 'react';
import { Verdict } from './duel-verdict';

type Props = {
  failure: Error | null;
  pairs: TDuelPair[] | undefined;
  counts: ReturnType<typeof pairCounts> | null;
  firstName: string;
  secondName: string;
  allVersions: boolean;
  onAllVersions: () => void;
};
export const DuelOutcome = ({
  failure,
  pairs,
  counts,
  firstName,
  secondName,
  allVersions,
  onAllVersions,
}: Props): JSX.Element => {
  if (failure) {
    return <p>Duel indisponible : {failure.message}</p>;
  }
  if (!pairs || !counts) {
    return <p>Chargement des paires…</p>;
  }
  return (
    <Verdict
      firstName={firstName}
      secondName={secondName}
      counts={counts}
      pairs={pairs.length}
      allVersions={allVersions}
      onAllVersions={onAllVersions}
    />
  );
};
