import { LiveStatus } from '@/components/controls/live-status';
import type { TLiveDot } from '@/components/controls/live-status';
import { sessionOptions } from '@/components/controls/session-options';
import { SessionSelect } from '@/components/controls/session-select';
import { VerifiedFilter } from '@/components/controls/verified-filter';
import { Masthead } from '@/components/masthead/masthead';
import { timeFormat } from '@/lib/format';
import { useMemo } from 'react';
import type { useClassementData } from './classement-data';

type TClassementStatus = {
  dot: TLiveDot;
  text: string;
};

function classementStatus(
  data: ReturnType<typeof useClassementData>,
  time: number | undefined,
): TClassementStatus {
  const { error, scope, teams, list, dataUpdatedAt, summary } = data;
  if (error) {
    return { dot: 'error', text: `Données indisponibles : ${error.message}` };
  }
  if (!scope) {
    return { dot: summary ? 'on' : '', text: 'Chargement des parties…' };
  }
  const count = teams.length > 0 ? list.length : scope.games;
  const duration = time === undefined || time === 0 ? '' : `, ${time} ms par coup`;
  const updatedAt = timeFormat.format(new Date(dataUpdatedAt));
  return { dot: 'on', text: `${count} parties${duration}, à jour à ${updatedAt}` };
}

type THeader = {
  data: ReturnType<typeof useClassementData>;
  time: number | undefined;
  verifiedParam: string | null;
  changeVerified: (value: boolean) => void;
  changeSession: (value: string) => void;
};

export function ClassementHeader({
  data,
  time,
  verifiedParam,
  changeVerified,
  changeSession,
}: THeader): React.JSX.Element {
  const { summary, session, pulses } = data;
  const { dot, text } = classementStatus(data, time);
  const indicator = useMemo(
    () => <LiveStatus dot={dot} text={text} pulses={pulses} />,
    [dot, text, pulses],
  );
  const options = useMemo(() => sessionOptions(summary?.sessions), [summary?.sessions]);
  return (
    <Masthead current="classement" status={indicator}>
      <VerifiedFilter
        hidden={!summary?.verified}
        checked={verifiedParam === '1'}
        onChange={changeVerified}
      />
      <SessionSelect value={session} options={options} onChange={changeSession} />
    </Masthead>
  );
}
