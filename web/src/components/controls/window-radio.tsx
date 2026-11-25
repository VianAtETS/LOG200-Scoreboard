import { useCallback } from 'react';
import type { ChangeEvent, FC } from 'react';

type Props = {
  id?: string;
  name: string;
  label: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
};

const WindowRadio: FC<Props> = ({ id, name, label, value, options, onChange }) => {
  const handleWindowChange = useCallback(
    (event: ChangeEvent<HTMLInputElement>): void => {
      onChange(event.target.value);
    },
    [onChange],
  );
  return (
    <div id={id} className="elo-window" role="radiogroup" aria-label={label}>
      {options.map((option) => (
        <label key={option.value}>
          <input
            type="radio"
            name={name}
            value={option.value}
            checked={option.value === value}
            onChange={handleWindowChange}
          />
          {option.label}
        </label>
      ))}
    </div>
  );
};

export { WindowRadio };
