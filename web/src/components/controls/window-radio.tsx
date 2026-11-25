import type { FC } from 'react';

type Props = {
  id?: string;
  name: string;
  label: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
};

const WindowRadio: FC<Props> = ({ id, name, label, value, options, onChange }) => (
  <div id={id} className="elo-window" role="radiogroup" aria-label={label}>
    {options.map((o) => (
      <label key={o.value}>
        <input
          type="radio"
          name={name}
          value={o.value}
          checked={o.value === value}
          onChange={() => onChange(o.value)}
        />
        {o.label}
      </label>
    ))}
  </div>
);

export { WindowRadio };
