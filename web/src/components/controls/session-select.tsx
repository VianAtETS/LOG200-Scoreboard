import { useCallback } from 'react';
import type { ChangeEvent, FC, ReactNode } from 'react';

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
  const handleSessionChange = useCallback(
    (event: ChangeEvent<HTMLSelectElement>): void => {
      onChange(event.target.value);
    },
    [onChange],
  );
  return (
    <label className="session">
      <span>{label}</span>
      <select id={id} value={value} disabled={disabled} onChange={handleSessionChange}>
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  );
};

export { SessionSelect };
