import { LiveStatus } from '@/components/controls/live-status';
import { sessionOptions } from '@/components/controls/session-options';
import { SessionSelect } from '@/components/controls/session-select';
import { VerifiedFilter } from '@/components/controls/verified-filter';
import { Masthead } from '@/components/masthead/masthead';
import { useMemo } from 'react';
import type { JSX } from 'react';
import type { useGraphData } from './graphe-data';

type Props = {
  status: ReturnType<typeof useGraphData>['status'];
  pulses: number;
  sessions: string[] | undefined;
  session: string;
  verified: boolean;
  onVerified: (checked: boolean) => void;
  onSession: (value: string) => void;
};
export const GraphHeader = ({
  status,
  pulses,
  sessions,
  session,
  verified,
  onVerified,
  onSession,
}: Props): JSX.Element => {
  const options = useMemo(() => sessionOptions(sessions), [sessions]);
  const indicator = useMemo(
    () => <LiveStatus dot={status.dot} text={status.text} pulses={pulses} />,
    [status.dot, status.text, pulses],
  );
  return (
    <Masthead current="graphe" status={indicator}>
      <VerifiedFilter hidden={!status.hasVerified} checked={verified} onChange={onVerified} />
      <SessionSelect value={session} options={options} onChange={onSession} />
    </Masthead>
  );
};
