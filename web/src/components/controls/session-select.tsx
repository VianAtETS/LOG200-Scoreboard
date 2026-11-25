import { GLOBAL } from '@/lib/constants';
import type { FC, ReactNode } from 'react';

type Props = {
  value: string;
  onChange: (value: string) => void;
  options: { value: string; label: ReactNode }[];
  label?: string;
  id?: string;
  disabled?: boolean;
};

const SessionSelect: FC<Props> = ({
  value,
  onChange,
  options,
  label = 'Session',
  id = 'session',
  disabled,
}) => {
  return (
    <label className="session">
      <span>{label}</span>
      <select id={id} value={value} disabled={disabled} onChange={(e) => onChange(e.target.value)}>
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
};

// "Toutes", puis chaque session.
export const sessionOptions = (sessions: string[] = []) => [
  { value: GLOBAL, label: 'Toutes' },
  ...sessions.map((s) => ({ value: s, label: s })),
];

export { SessionSelect };
