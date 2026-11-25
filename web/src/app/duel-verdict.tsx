import { SPRT_EXPECTED, eloGap, signTest, sprt, wilson } from '@/lib/duel';
import type { pairCounts } from '@/lib/duel';
import type { JSX } from 'react';
import { signed } from '@/lib/format';

const percent = (value: number): string => `${Math.round(100 * value)} %`;
const plural = (count: number, word: string): string => `${count} ${word}${count > 1 ? 's' : ''}`;
const MIN_DISPLAY_P = 0.001;

type VerdictProps = {
  firstName: string;
  secondName: string;
  counts: ReturnType<typeof pairCounts>;
  pairs: number;
  allVersions: boolean;
  onAllVersions: () => void;
};

// Verdict du SPRT, puis le détail d'isola : paires, test du signe, score et Elo de la première équipe.
export const Verdict = ({
  firstName,
  secondName,
  counts,
  pairs,
  allVersions,
  onAllVersions,
}: VerdictProps): JSX.Element => {
  const decisive = counts.a + counts.b;
  const verdict = sprt(counts.a, counts.b);
  const probability = signTest(counts.a, counts.b);
  const games = 2 * pairs;
  const wins = 2 * counts.a + counts.split;
  const score = games ? wins / games : Number.NaN;
  const [low, high] = wilson(wins, games);
  const elo = eloGap(score);
  return (
    <>
      <p className="duel-verdict">
        {verdict === 'a' && `${firstName} est meilleur.`}
        {verdict === 'b' && `${secondName} est meilleur.`}
        {verdict === 'close' && 'Pas de grand écart entre les deux.'}
        {verdict === 'undecided' &&
          `Indécis : ${plural(decisive, 'paire')} ${decisive > 1 ? 'décisives' : 'décisive'}, environ ${SPRT_EXPECTED} nécessaires.`}
        {verdict === 'undecided' && !allVersions && (
          <button type="button" className="share" onClick={onAllVersions}>
            Toutes les versions
          </button>
        )}
      </p>
      <ul className="duel-detail">
        <li>
          {plural(pairs, 'paire')} : {counts.a} gagnées 2-0 par {firstName}, {counts.b} par{' '}
          {secondName}, {counts.split} à 1-1 (décidées par l&apos;ouverture)
        </li>
        {games > 0 && (
          <li>
            Score de {firstName} : {percent(score)} des parties (IC 95 % : {percent(low)} à{' '}
            {percent(high)}), écart Elo {Number.isFinite(elo) ? signed(Math.round(elo)) : '–'}
          </li>
        )}
        {decisive > 0 && (
          <li>
            Test du signe sur les paires 2-0 : p{' '}
            {probability < MIN_DISPLAY_P
              ? '< 0,001'
              : `= ${probability.toLocaleString('fr-CA', { maximumFractionDigits: 3 })}`}
          </li>
        )}
      </ul>
    </>
  );
};
