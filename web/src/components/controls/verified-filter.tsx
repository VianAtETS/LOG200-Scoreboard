import { useCallback } from 'react';
import type { ChangeEvent, FC } from 'react';

type Props = {
  hidden: boolean;
  checked: boolean;
  onChange: (checked: boolean) => void;
};

// Case "Vérifiées seulement", gardée dans ?verified=1. N'apparaît qu'une fois une partie distante jouée.
const VerifiedFilter: FC<Props> = ({ hidden, checked, onChange }) => {
  const handleVerifiedChange = useCallback(
    (event: ChangeEvent<HTMLInputElement>): void => {
      onChange(event.target.checked);
    },
    [onChange],
  );
  return (
    <label
      id="verified-filter"
      className="verified"
      title="Sans les parties distantes : l'Elo est recalculé sur les seules parties jouées sur le serveur"
      hidden={hidden}
    >
      <input id="verified" type="checkbox" checked={checked} onChange={handleVerifiedChange} />
      <span>Vérifiées seulement</span>
    </label>
  );
};

export { VerifiedFilter };
